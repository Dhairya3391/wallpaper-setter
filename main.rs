use fast_image_resize as fr;
use fast_image_resize::images::Image;
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions};
use image::{imageops, GenericImageView, ImageBuffer, Rgb};
use rand::seq::SliceRandom;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

// ── state files (all in ~/.cache/) ───────────────────────────────────────────
// wall_cfg    →  line 1: "WIDTH HEIGHT"   line 2: "/walls/dir"
// wall_queue  →  pre-shuffled paths, one per line; pop front each run

fn cache_dir() -> PathBuf {
    dirs::cache_dir().unwrap_or_else(|| dirs::home_dir().unwrap().join(".cache"))
}

fn prompt(q: &str) -> String {
    print!("{q} ");
    std::io::stdout().flush().ok();
    let mut s = String::new();
    std::io::stdin().read_line(&mut s).ok();
    s.trim().to_owned()
}

// ── config ────────────────────────────────────────────────────────────────────

struct Cfg {
    screen_w: u32,
    screen_h: u32,
    walls_dir: PathBuf,
}

fn load_cfg() -> Cfg {
    let cfg_file = cache_dir().join("wall_cfg");

    if let Ok(f) = fs::File::open(&cfg_file) {
        let mut lines = BufReader::new(f).lines().map_while(Result::ok);
        let dims = lines.next().unwrap_or_default();
        let dir  = lines.next().unwrap_or_default();
        let mut parts = dims.splitn(2, ' ');
        let w: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let h: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        if w > 0 && h > 0 && !dir.is_empty() {
            return Cfg { screen_w: w, screen_h: h, walls_dir: PathBuf::from(dir) };
        }
    }

    // first run
    let w: u32 = prompt("Screen width  (px):").parse().unwrap_or(1080);
    let h: u32 = prompt("Screen height (px):").parse().unwrap_or(2400);
    let raw = prompt("Wallpaper directory:");
    let walls_dir = if raw.starts_with('~') {
        dirs::home_dir()
            .map(|home| home.join(raw.trim_start_matches("~/")))
            .unwrap_or_else(|| PathBuf::from(&raw))
    } else {
        PathBuf::from(&raw)
    };

    let _ = fs::create_dir_all(cache_dir());
    if let Ok(mut f) = fs::File::create(&cfg_file) {
        let _ = writeln!(f, "{w} {h}");
        let _ = writeln!(f, "{}", walls_dir.display());
    }

    Cfg { screen_w: w, screen_h: h, walls_dir }
}

// ── non-repeat queue ──────────────────────────────────────────────────────────
// Pre-shuffled list of all wallpaper paths. Each run pops the first entry.
// When the file is empty, rebuild it with a fresh shuffle of everything.

fn all_wallpapers(dir: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map_or(false, |x| {
                    matches!(x.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp")
                })
        })
        .map(|e| e.into_path())
        .collect()
}

fn pick_wallpaper(walls_dir: &Path) -> Option<PathBuf> {
    let queue_path = cache_dir().join("wall_queue");

    // Try to pop the first line from the queue
    if let Ok(f) = fs::File::open(&queue_path) {
        let mut lines: Vec<String> = BufReader::new(f)
            .lines()
            .filter_map(Result::ok)
            .filter(|l| !l.is_empty())
            .collect();

        if !lines.is_empty() {
            let chosen = PathBuf::from(lines.remove(0));
            // Rewrite queue without the consumed entry
            if let Ok(mut f) = fs::File::create(&queue_path) {
                for l in &lines {
                    let _ = writeln!(f, "{l}");
                }
            }
            return Some(chosen);
        }
    }

    // Queue empty or missing — rebuild with a fresh shuffle
    let mut all = all_wallpapers(walls_dir);
    if all.is_empty() {
        return None;
    }
    all.shuffle(&mut rand::thread_rng());

    let chosen = all.remove(0);

    let _ = fs::create_dir_all(cache_dir());
    if let Ok(mut f) = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&queue_path)
    {
        for p in &all {
            let _ = writeln!(f, "{}", p.display());
        }
    }

    Some(chosen)
}

// ── image processing ──────────────────────────────────────────────────────────

/// object-fit: cover — scale to fill screen, center-crop the excess.
fn cover_fit(iw: u32, ih: u32, sw: u32, sh: u32) -> (u32, u32, u32, u32) {
    let scale = (sw as f64 / iw as f64).max(sh as f64 / ih as f64);
    let scaled_w = (iw as f64 * scale).round() as u32;
    let scaled_h = (ih as f64 * scale).round() as u32;
    let cx = (scaled_w.saturating_sub(sw)) / 2;
    let cy = (scaled_h.saturating_sub(sh)) / 2;
    (scaled_w, scaled_h, cx, cy)
}

fn process(
    path: &Path,
    sw: u32,
    sh: u32,
) -> Result<ImageBuffer<Rgb<u8>, Vec<u8>>, Box<dyn std::error::Error>> {
    let img = image::open(path)?;
    let (iw, ih) = img.dimensions();
    let (scaled_w, scaled_h, cx, cy) = cover_fit(iw, ih, sw, sh);

    let src = Image::from_vec_u8(iw, ih, img.to_rgba8().into_raw(), fr::PixelType::U8x4)?;
    let mut dst = Image::new(scaled_w, scaled_h, fr::PixelType::U8x4);
    fr::Resizer::new().resize(
        &src,
        &mut dst,
        &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3)),
    )?;

    let rgba: image::RgbaImage =
        image::ImageBuffer::from_raw(scaled_w, scaled_h, dst.into_vec())
            .ok_or("buffer size mismatch")?;
    let cropped = imageops::crop_imm(&rgba, cx, cy, sw, sh).to_image();

    Ok(ImageBuffer::from_fn(sw, sh, |x, y| {
        let p = cropped.get_pixel(x, y);
        Rgb([p[0], p[1], p[2]])
    }))
}

// ── set wallpaper ─────────────────────────────────────────────────────────────

fn set_wallpaper(img: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = cache_dir().join("wall_tmp.png");
    img.save(&tmp)?;
    // Both commands reference the same file → identical image on home + lock
    Command::new("termux-wallpaper").arg("-f").arg(&tmp).status()?;
    Command::new("termux-wallpaper").arg("-f").arg(&tmp).arg("-l").status()?;
    Ok(())
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    let cfg = load_cfg();

    let Some(path) = pick_wallpaper(&cfg.walls_dir) else {
        eprintln!("No wallpapers found in {}", cfg.walls_dir.display());
        std::process::exit(1);
    };

    match process(&path, cfg.screen_w, cfg.screen_h) {
        Ok(img) => match set_wallpaper(&img) {
            Ok(()) => println!("OK  {}", path.display()),
            Err(e) => eprintln!("set_wallpaper: {e}"),
        },
        Err(e) => eprintln!("process {:?}: {e}", path),
    }
}

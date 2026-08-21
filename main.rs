use fast_image_resize as fr;
use fast_image_resize::images::Image;
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions};
use image::{GenericImageView, ImageBuffer, Rgb, imageops};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const CONFIG_FILE: &str = ".config/wallpaper_manager.json";

#[derive(Serialize, Deserialize, Default)]
struct Config {
    used_wallpapers: HashSet<String>,
    current_wallpaper: Option<String>,
    wallpaper_dir: Option<String>,
    screen_width: Option<u32>,
    screen_height: Option<u32>,
}

fn load_config() -> Config {
    dirs::home_dir()
        .and_then(|home| File::open(home.join(CONFIG_FILE)).ok())
        .map(|file| serde_json::from_reader(BufReader::new(file)).unwrap_or_default())
        .unwrap_or_default()
}

fn save_config(config: &Config) {
    if let Some(path) = dirs::home_dir().map(|h| h.join(CONFIG_FILE)) {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(file) = File::create(path) {
            let _ = serde_json::to_writer(file, config);
        }
    }
}

fn prompt(question: &str) -> String {
    print!("{question} ");
    std::io::stdout().flush().ok();
    let mut buf = String::new();
    std::io::stdin().read_line(&mut buf).ok();
    buf.trim().to_owned()
}

// ── screen size ──────────────────────────────────────────────────────────────

fn detect_from_fb0_modes() -> Option<(u32, u32)> {
    let raw = fs::read_to_string("/sys/class/graphics/fb0/modes").ok()?;
    let line = raw.lines().next()?;
    let res = line.split(':').nth(1)?.split('-').next()?;
    let mut dims = res.split('x');
    let w: u32 = dims.next()?.trim().parse().ok()?;
    let h: u32 = dims.next()?.trim().parse().ok()?;
    Some((w, h))
}

fn detect_from_fb0() -> Option<(u32, u32)> {
    let raw = fs::read_to_string("/sys/class/graphics/fb0/virtual_size").ok()?;
    let mut parts = raw.trim().splitn(2, ',');
    let w: u32 = parts.next()?.trim().parse().ok()?;
    let h: u32 = parts.next()?.trim().parse().ok()?;
    let h = if h > w * 4 { h / 2 } else { h };
    Some((w, h))
}

fn get_screen_size(config: &mut Config) -> (u32, u32) {
    if let (Some(w), Some(h)) = (config.screen_width, config.screen_height) {
        return (w, h);
    }

    let detected = detect_from_fb0_modes().or_else(detect_from_fb0);

    let (w, h) = if let Some(dims @ (w, h)) = detected {
        if w >= 100 && h >= 100 {
            dims
        } else {
            eprintln!("Detected implausible resolution ({w}x{h}), please enter manually.");
            let w: u32 = prompt("Screen width (px):").parse().unwrap_or(1080);
            let h: u32 = prompt("Screen height (px):").parse().unwrap_or(2400);
            (w, h)
        }
    } else {
        println!("Could not detect screen resolution automatically.");
        let w: u32 = prompt("Screen width (px):").parse().unwrap_or(1080);
        let h: u32 = prompt("Screen height (px):").parse().unwrap_or(2400);
        (w, h)
    };

    config.screen_width = Some(w);
    config.screen_height = Some(h);
    (w, h)
}

// ── wallpaper directory ───────────────────────────────────────────────────────

fn get_wallpaper_dir(config: &mut Config) -> PathBuf {
    if let Some(dir) = &config.wallpaper_dir {
        return PathBuf::from(dir);
    }

    let input = prompt("Wallpaper directory path:");
    let path = if input.starts_with('~') {
        dirs::home_dir()
            .map(|h| h.join(input.trim_start_matches("~/")))
            .unwrap_or_else(|| PathBuf::from(&input))
    } else {
        PathBuf::from(&input)
    };

    config.wallpaper_dir = Some(path.to_string_lossy().into_owned());
    path
}

fn get_all_wallpapers(dir: &Path) -> Vec<PathBuf> {
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

// ── image processing ──────────────────────────────────────────────────────────

/// CSS object-fit:cover — scale so image fills the screen, center-crop the excess.
fn cover_fit(img_w: u32, img_h: u32, screen_w: u32, screen_h: u32) -> (u32, u32, u32, u32) {
    let scale = (screen_w as f64 / img_w as f64).max(screen_h as f64 / img_h as f64);
    let scaled_w = (img_w as f64 * scale).round() as u32;
    let scaled_h = (img_h as f64 * scale).round() as u32;
    let crop_x = (scaled_w.saturating_sub(screen_w)) / 2;
    let crop_y = (scaled_h.saturating_sub(screen_h)) / 2;
    (scaled_w, scaled_h, crop_x, crop_y)
}

fn process_wallpaper(
    input_path: &Path,
    screen_w: u32,
    screen_h: u32,
) -> Result<ImageBuffer<Rgb<u8>, Vec<u8>>, Box<dyn std::error::Error>> {
    let img = image::open(input_path)?;
    let (img_w, img_h) = img.dimensions();
    let (scaled_w, scaled_h, crop_x, crop_y) = cover_fit(img_w, img_h, screen_w, screen_h);

    let rgba = img.to_rgba8();
    let src = Image::from_vec_u8(img_w, img_h, rgba.into_raw(), fr::PixelType::U8x4)?;
    let mut dst = Image::new(scaled_w, scaled_h, fr::PixelType::U8x4);

    let mut resizer = fr::Resizer::new();
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    resizer.resize(&src, &mut dst, &options)?;

    let rgba_img: image::RgbaImage =
        image::ImageBuffer::from_raw(scaled_w, scaled_h, dst.into_vec())
            .ok_or("buffer size mismatch")?;

    let cropped = imageops::crop_imm(&rgba_img, crop_x, crop_y, screen_w, screen_h).to_image();

    Ok(ImageBuffer::from_fn(screen_w, screen_h, |x, y| {
        let p = cropped.get_pixel(x, y);
        Rgb([p[0], p[1], p[2]])
    }))
}

// ── wallpaper application ─────────────────────────────────────────────────────

fn set_wallpaper(image: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> Result<(), Box<dyn std::error::Error>> {
    let tmp = std::env::temp_dir().join("wallpaper_tmp.png");
    image.save(&tmp)?;

    let run = |extra: &[&str]| {
        Command::new("termux-wallpaper")
            .arg("-f")
            .arg(&tmp)
            .args(extra)
            .status()
    };
    run(&[])?;
    run(&["-l"])?;

    let _ = fs::remove_file(&tmp);
    Ok(())
}

// ── selection ─────────────────────────────────────────────────────────────────

fn pick_wallpaper(config: &mut Config, dir: &Path) -> Option<PathBuf> {
    let all = get_all_wallpapers(dir);
    if all.is_empty() {
        return None;
    }

    let mut rng = rand::thread_rng();
    let key = |p: &PathBuf| p.to_string_lossy().into_owned();

    let unused: Vec<_> = all
        .iter()
        .filter(|p| !config.used_wallpapers.contains(&key(p)))
        .cloned()
        .collect();

    let chosen = if unused.is_empty() {
        config.used_wallpapers.clear();
        all.choose(&mut rng).cloned()
    } else {
        unused.choose(&mut rng).cloned()
    }?;

    config.used_wallpapers.insert(key(&chosen));
    config.current_wallpaper = Some(key(&chosen));
    Some(chosen)
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    let mut config = load_config();

    let (screen_w, screen_h) = get_screen_size(&mut config);
    let dir = get_wallpaper_dir(&mut config);

    let Some(path) = pick_wallpaper(&mut config, &dir) else {
        eprintln!("No wallpapers found in {}", dir.display());
        std::process::exit(1);
    };

    match process_wallpaper(&path, screen_w, screen_h) {
        Ok(img) => match set_wallpaper(&img) {
            Ok(()) => {
                save_config(&config);
                println!("OK  {}  ({}x{})", path.display(), screen_w, screen_h);
            }
            Err(e) => eprintln!("set_wallpaper: {e}"),
        },
        Err(e) => eprintln!("process {:?}: {e}", path),
    }
}

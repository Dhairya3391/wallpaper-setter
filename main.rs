use fast_image_resize as fr;
use fast_image_resize::images::Image;
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions};
use image::{GenericImageView, ImageBuffer, Rgb, imageops};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;

const CONFIG_FILE: &str = ".config/wallpaper_manager.json";
const WALLPAPER_DIR: &str = "storage/shared/.Walls";
const TEMP_WALLPAPER: &str = "/data/data/com.termux/files/home/temp_wallpaper.png";

#[derive(Serialize, Deserialize, Default)]
struct Config {
    used_wallpapers: HashSet<String>,
    current_wallpaper: Option<String>,
    /// Cached screen dimensions — populated once at first run
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
    if let Some(config_path) = dirs::home_dir().map(|home| home.join(CONFIG_FILE)) {
        if let Some(parent) = config_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(file) = File::create(config_path) {
            let _ = serde_json::to_writer(file, config);
        }
    }
}

/// Try /sys/class/graphics/fb0/virtual_size  →  "WxH"
fn detect_from_fb0() -> Option<(u32, u32)> {
    let raw = fs::read_to_string("/sys/class/graphics/fb0/virtual_size").ok()?;
    let raw = raw.trim();
    let mut parts = raw.splitn(2, ',');
    let w: u32 = parts.next()?.trim().parse().ok()?;
    let h: u32 = parts.next()?.trim().parse().ok()?;
    // virtual_size can report doubled height (front+back buffer) — halve if taller than 4:1
    let h = if h > w * 4 { h / 2 } else { h };
    Some((w, h))
}

/// Try /sys/class/graphics/fb0/modes  →  "U:WxH-Hz"
fn detect_from_fb0_modes() -> Option<(u32, u32)> {
    let raw = fs::read_to_string("/sys/class/graphics/fb0/modes").ok()?;
    // First mode line: "U:1080x2400p-60"
    let line = raw.lines().next()?;
    let res_part = line.split(':').nth(1)?.split('-').next()?;
    let mut dims = res_part.split('x');
    let w: u32 = dims.next()?.trim().parse().ok()?;
    let h: u32 = dims.next()?.trim().parse().ok()?;
    Some((w, h))
}

/// Ask the user once; remember in config so we never prompt again.
fn prompt_screen_size() -> (u32, u32) {
    eprintln!("Could not detect screen resolution automatically.");
    eprintln!("Enter width (e.g. 1080): ");
    let mut w_str = String::new();
    std::io::stdin().read_line(&mut w_str).ok();
    eprintln!("Enter height (e.g. 2400): ");
    let mut h_str = String::new();
    std::io::stdin().read_line(&mut h_str).ok();
    let w = w_str.trim().parse().unwrap_or(1080);
    let h = h_str.trim().parse().unwrap_or(2400);
    (w, h)
}

fn get_screen_size(config: &mut Config) -> (u32, u32) {
    // Use cached value if available
    if let (Some(w), Some(h)) = (config.screen_width, config.screen_height) {
        return (w, h);
    }

    let dims = detect_from_fb0_modes()
        .or_else(detect_from_fb0)
        .unwrap_or_else(|| prompt_screen_size());

    // Sanity check: both dimensions must be plausible phone sizes
    let (w, h) = if dims.0 >= 100 && dims.1 >= 100 {
        dims
    } else {
        eprintln!("Detected implausible dimensions ({} x {}), falling back to prompt.", dims.0, dims.1);
        prompt_screen_size()
    };

    config.screen_width = Some(w);
    config.screen_height = Some(h);
    (w, h)
}

fn get_all_wallpapers() -> Vec<PathBuf> {
    let Some(dir) = dirs::home_dir().map(|h| h.join(WALLPAPER_DIR)) else {
        return Vec::new();
    };
    walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|ext| ext.to_str())
                .map_or(false, |ext| {
                    matches!(ext.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp")
                })
        })
        .map(|e| e.into_path())
        .collect()
}

/// Cover-fit: scale image so it *covers* the screen (no empty bars),
/// then center-crop to exact screen dimensions.
///
/// Scale factor = max(screen_w / img_w, screen_h / img_h)
/// This is the "object-fit: cover" CSS equivalent — maximises visible
/// image area while always filling the full screen.
fn cover_fit(img_w: u32, img_h: u32, screen_w: u32, screen_h: u32) -> (u32, u32, u32, u32) {
    let scale_x = screen_w as f64 / img_w as f64;
    let scale_y = screen_h as f64 / img_h as f64;
    let scale = scale_x.max(scale_y); // cover: take the larger scale

    let scaled_w = (img_w as f64 * scale).round() as u32;
    let scaled_h = (img_h as f64 * scale).round() as u32;

    // Center-crop offsets
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

    // fast_image_resize wants RGBA u8x4
    let rgba = img.to_rgba8();
    let src = Image::from_vec_u8(img_w, img_h, rgba.into_raw(), fr::PixelType::U8x4)?;

    let mut dst = Image::new(scaled_w, scaled_h, fr::PixelType::U8x4);

    let mut resizer = fr::Resizer::new();
    // Lanczos3: best sharpness for downscaling; also good for upscaling
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    resizer.resize(&src, &mut dst, &options)?;

    // Convert RGBA buffer → image::RgbaImage, then crop, then strip alpha
    let rgba_img: image::RgbaImage =
        image::ImageBuffer::from_raw(scaled_w, scaled_h, dst.into_vec())
            .ok_or("buffer size mismatch")?;

    // imageops::crop returns a SubImage view — convert to owned
    let cropped = imageops::crop_imm(&rgba_img, crop_x, crop_y, screen_w, screen_h).to_image();

    // Drop alpha channel — termux-wallpaper expects RGB / JPEG-friendly
    let rgb: ImageBuffer<Rgb<u8>, Vec<u8>> =
        ImageBuffer::from_fn(screen_w, screen_h, |x, y| {
            let p = cropped.get_pixel(x, y);
            Rgb([p[0], p[1], p[2]])
        });

    Ok(rgb)
}

fn set_wallpaper(image: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> Result<(), Box<dyn std::error::Error>> {
    image.save(TEMP_WALLPAPER)?;

    let run = |extra: &[&str]| {
        Command::new("termux-wallpaper")
            .arg("-f")
            .arg(TEMP_WALLPAPER)
            .args(extra)
            .status()
    };

    run(&[])?;       // home screen
    run(&["-l"])?;   // lock screen

    let _ = fs::remove_file(TEMP_WALLPAPER);
    Ok(())
}

fn pick_wallpaper(config: &mut Config) -> Option<PathBuf> {
    let all = get_all_wallpapers();
    if all.is_empty() {
        return None;
    }

    let mut rng = rand::thread_rng();

    let unused: Vec<_> = all
        .iter()
        .filter(|p| !config.used_wallpapers.contains(&p.to_string_lossy().into_owned()))
        .cloned()
        .collect();

    let chosen = if unused.is_empty() {
        config.used_wallpapers.clear();
        all.choose(&mut rng).cloned()
    } else {
        unused.choose(&mut rng).cloned()
    }?;

    config.used_wallpapers.insert(chosen.to_string_lossy().into_owned());
    config.current_wallpaper = Some(chosen.to_string_lossy().into_owned());
    Some(chosen)
}

fn main() {
    let mut config = load_config();
    let (screen_w, screen_h) = get_screen_size(&mut config);

    let Some(path) = pick_wallpaper(&mut config) else {
        eprintln!("No wallpapers found in ~/{}", WALLPAPER_DIR);
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
        Err(e) => eprintln!("process_wallpaper {:?}: {e}", path),
    }
}

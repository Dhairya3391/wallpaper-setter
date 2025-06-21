use image::{GenericImageView, ImageBuffer, Rgb};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::collections::HashSet;
use fast_image_resize as fr;
use fast_image_resize::images::Image;

const CONFIG_FILE: &str = ".config/wallpaper_manager.json";
const WALLPAPER_DIR: &str = "storage/shared/.Walls";
const DEVICE_WIDTH: u32 = 1080;
const DEVICE_HEIGHT: u32 = 2400;
const TEMP_WALLPAPER: &str = "/data/data/com.termux/files/home/temp_wallpaper.png";

#[derive(Serialize, Deserialize, Default)]
struct Config {
    used_wallpapers: HashSet<String>,
    favorites: Vec<String>,
    current_wallpaper: Option<String>,
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

fn get_all_wallpapers() -> Vec<PathBuf> {
    dirs::home_dir()
        .map(|home| home.join(WALLPAPER_DIR))
        .map(|dir| {
            walkdir::WalkDir::new(dir)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| {
                    e.path()
                        .extension()
                        .and_then(|ext| ext.to_str())
                        .map_or(false, |ext| matches!(ext.to_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp"))
                })
                .map(|e| e.path().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn calculate_dimensions(img_width: u32, img_height: u32) -> (u32, u32) {
    let img_aspect_ratio = img_width as f32 / img_height as f32;
    let device_aspect_ratio = DEVICE_WIDTH as f32 / DEVICE_HEIGHT as f32;

    let similar_ratio_threshold = 0.05;
    let ultra_wide_threshold = 2.2;
    let ultra_tall_threshold = 0.5;

    match img_aspect_ratio {
        ratio if ratio > ultra_wide_threshold => {
            let new_height = DEVICE_HEIGHT;
            let new_width = (new_height as f32 * ratio) as u32;
            (new_width, new_height)
        },
        ratio if ratio < ultra_tall_threshold => {
            let new_width = DEVICE_WIDTH;
            let new_height = (new_width as f32 / ratio) as u32;
            (new_width, new_height)
        },
        ratio if (ratio - device_aspect_ratio).abs() < similar_ratio_threshold => {
            (DEVICE_WIDTH, DEVICE_HEIGHT)
        },
        ratio if ratio > device_aspect_ratio => {
            let new_height = DEVICE_HEIGHT;
            let new_width = (new_height as f32 * ratio) as u32;
            // If the calculated width is still less than device width,
            // scale based on width instead to ensure coverage.
             if new_width < DEVICE_WIDTH {
                (DEVICE_WIDTH, (DEVICE_WIDTH as f32 / ratio) as u32)
            } else {
                (new_width, new_height)
            }
        },
        ratio => { // This case handles ratio <= device_aspect_ratio
            let new_width = DEVICE_WIDTH;
            let new_height = (new_width as f32 / ratio) as u32;
             // If the calculated height is still less than device height,
            // scale based on height instead to ensure coverage.
            if new_height < DEVICE_HEIGHT {
                 ((DEVICE_HEIGHT as f32 * ratio) as u32, DEVICE_HEIGHT)
            } else {
                (new_width, new_height)
            }
        },
    }
}

fn process_wallpaper(input_path: &Path) -> Result<ImageBuffer<Rgb<u8>, Vec<u8>>, image::ImageError> {
    let img = image::open(input_path)?;
    let (img_width, img_height) = img.dimensions();
    let (new_width, new_height) = calculate_dimensions(img_width, img_height);

    let img_rgba = img.to_rgba8();
    let input_image = Image::from_vec_u8(
        img_width,
        img_height,
        img_rgba.into_raw(),
        fr::PixelType::U8x4,
    ).unwrap(); // Consider error handling here as well

    let mut output_image = Image::new(
        new_width,
        new_height,
        input_image.pixel_type(),
    );

    let mut resizer = fr::Resizer::new();
    let options = fr::ResizeOptions::new();

    // Pass references to the Image structs directly, as they implement the necessary traits
    resizer.resize(&input_image, &mut output_image, &options).unwrap(); // Consider error handling

    let mut output = ImageBuffer::new(DEVICE_WIDTH, DEVICE_HEIGHT);
    let x_offset = (new_width.saturating_sub(DEVICE_WIDTH)) / 2;
    let y_offset = (new_height.saturating_sub(DEVICE_HEIGHT)) / 2;

    let output_buffer = output_image.buffer();
    let output_width = output_image.width(); // Use the potentially larger output image dimensions

    for (x, y, pixel) in output.enumerate_pixels_mut() {
        let source_x = x.saturating_add(x_offset);
        let source_y = y.saturating_add(y_offset);
        if source_x < new_width && source_y < new_height {
            let idx = (source_y * output_width + source_x) as usize * 4;
             // Ensure we don't go out of bounds for the output_buffer
            if idx + 3 < output_buffer.len() {
                 *pixel = Rgb([
                    output_buffer[idx],
                    output_buffer[idx + 1],
                    output_buffer[idx + 2],
                ]);
            } else {
                // Handle potential out-of-bounds access if needed,
                // e.g., set a default color or log a warning.
                // For now, we just skip assigning the pixel.
            }
        }
    }

    Ok(output)
}

fn set_wallpaper(image: &ImageBuffer<Rgb<u8>, Vec<u8>>) {
    let _ = image.save(TEMP_WALLPAPER);

    // Set for home screen
    let _ = Command::new("termux-wallpaper")
        .args(&["-f", TEMP_WALLPAPER])
        .status();
    // Set for lock screen
    let _ = Command::new("termux-wallpaper")
        .args(&["-f", TEMP_WALLPAPER, "-l"])
        .status();

    let _ = fs::remove_file(TEMP_WALLPAPER);
}

fn random_wallpaper(config: &mut Config) -> Option<PathBuf> {
    let all_wallpapers = get_all_wallpapers();
    if all_wallpapers.is_empty() {
        return None;
    }

    let unused_wallpapers: Vec<_> = all_wallpapers
        .iter()
        .filter(|wp| !config.used_wallpapers.contains(&wp.to_string_lossy().to_string()))
        .cloned()
        .collect();

    let chosen_wallpaper = if unused_wallpapers.is_empty() {
        // If all wallpapers have been used, clear the used list and pick randomly from all.
        config.used_wallpapers.clear();
        all_wallpapers.choose(&mut rand::thread_rng()).cloned()
    } else {
        unused_wallpapers.choose(&mut rand::thread_rng()).cloned()
    };

    if let Some(wp) = &chosen_wallpaper {
        config.used_wallpapers.insert(wp.to_string_lossy().to_string());
        config.current_wallpaper = Some(wp.to_string_lossy().to_string());
    }

    chosen_wallpaper
}

fn main() {
    let mut config = load_config();

    if let Some(wallpaper_path) = random_wallpaper(&mut config) {
        match process_wallpaper(&wallpaper_path) {
            Ok(processed_image) => {
                set_wallpaper(&processed_image);
                save_config(&config);
                println!("Wallpaper set successfully: {:?}", wallpaper_path);
            }
            Err(e) => {
                eprintln!("Error processing wallpaper {:?}: {}", wallpaper_path, e);
            }
        }
    } else {
        eprintln!("No wallpapers found.");
    }
}

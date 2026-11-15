//! Image processing — decode, smart crop, cover-fit, and contain-blur.

use fast_image_resize as fr;
use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions};
use image::{imageops, GenericImageView, RgbImage};
use std::io::Cursor;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Symmetrical cover fit: scale to fill screen, then center-crop the excess equally.
pub fn cover_fit(image_w: u32, image_h: u32, screen_w: u32, screen_h: u32) -> (u32, u32, u32, u32) {
    let scale = (screen_w as f64 / image_w as f64).max(screen_h as f64 / image_h as f64);
    let scaled_w = (image_w as f64 * scale).round() as u32;
    let scaled_h = (image_h as f64 * scale).round() as u32;
    let crop_x = scaled_w.saturating_sub(screen_w) / 2;
    let crop_y = scaled_h.saturating_sub(screen_h) / 2;
    (scaled_w, scaled_h, crop_x, crop_y)
}

/// Smart cover fit: centers horizontally, but anchors vertical crop at 35% from the top
/// to avoid cutting off subjects, heads, or celestial skies in portrait phone wallpapers.
pub fn smart_cover_fit(image_w: u32, image_h: u32, screen_w: u32, screen_h: u32) -> (u32, u32, u32, u32) {
    let scale = (screen_w as f64 / image_w as f64).max(screen_h as f64 / image_h as f64);
    let scaled_w = (image_w as f64 * scale).round() as u32;
    let scaled_h = (image_h as f64 * scale).round() as u32;
    let crop_x = scaled_w.saturating_sub(screen_w) / 2;

    let excess_y = scaled_h.saturating_sub(screen_h);
    let crop_y = (excess_y as f64 * 0.35).round() as u32;
    (scaled_w, scaled_h, crop_x, crop_y)
}

/// Resize raw RGB buffer using Lanczos3 convolution.
fn resize_rgb(
    src_raw: &[u8],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
) -> Result<RgbImage> {
    let source = ImageRef::new(src_w, src_h, src_raw, fr::PixelType::U8x3)?;
    let mut resized = Image::new(dst_w, dst_h, fr::PixelType::U8x3);

    fr::Resizer::new().resize(
        &source,
        &mut resized,
        &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3)),
    )?;

    RgbImage::from_raw(dst_w, dst_h, resized.into_vec())
        .ok_or_else(|| "resized image buffer has an unexpected size".into())
}

/// Process image using specified mode: "smart", "cover", or "contain_blur".
pub fn process_image(img: image::DynamicImage, screen_w: u32, screen_h: u32, mode: &str) -> Result<RgbImage> {
    let (image_w, image_h) = img.dimensions();

    match mode {
        "contain_blur" => {
            // 1. Calculate contain scale (fits entirely on screen)
            let scale = (screen_w as f64 / image_w as f64).min(screen_h as f64 / image_h as f64);
            let inner_w = (image_w as f64 * scale).round() as u32;
            let inner_h = (image_h as f64 * scale).round() as u32;

            let rgb = img.into_rgb8();

            // 2. Generate ambient backdrop: downscale to thumbnail then upscale to full screen
            // producing a silky smooth Gaussian-like blur with zero performance cost
            let thumb_w = (screen_w / 20).max(32);
            let thumb_h = (screen_h / 20).max(64);
            let thumb = resize_rgb(&rgb, image_w, image_h, thumb_w, thumb_h)?;
            let mut backdrop = resize_rgb(&thumb, thumb_w, thumb_h, screen_w, screen_h)?;

            // Dim backdrop slightly (60% brightness) so the main artwork pops
            for p in backdrop.pixels_mut() {
                p.0[0] = (p.0[0] as f32 * 0.6) as u8;
                p.0[1] = (p.0[1] as f32 * 0.6) as u8;
                p.0[2] = (p.0[2] as f32 * 0.6) as u8;
            }

            // 3. Resize main image cleanly
            let inner = resize_rgb(&rgb, image_w, image_h, inner_w, inner_h)?;

            // 4. Composite main image centered onto the blurred backdrop
            let offset_x = (screen_w - inner_w) / 2;
            let offset_y = (screen_h - inner_h) / 2;
            imageops::overlay(&mut backdrop, &inner, offset_x as i64, offset_y as i64);

            Ok(backdrop)
        }
        "cover" => {
            let (scaled_w, scaled_h, crop_x, crop_y) = cover_fit(image_w, image_h, screen_w, screen_h);
            let rgb = img.into_rgb8();
            let resized = resize_rgb(&rgb, image_w, image_h, scaled_w, scaled_h)?;
            Ok(imageops::crop_imm(&resized, crop_x, crop_y, screen_w, screen_h).to_image())
        }
        _ => {
            // Default "smart": intelligent vertical bias
            let (scaled_w, scaled_h, crop_x, crop_y) = smart_cover_fit(image_w, image_h, screen_w, screen_h);
            let rgb = img.into_rgb8();
            let resized = resize_rgb(&rgb, image_w, image_h, scaled_w, scaled_h)?;
            Ok(imageops::crop_imm(&resized, crop_x, crop_y, screen_w, screen_h).to_image())
        }
    }
}

/// Process image from file path.
pub fn process_file(path: &std::path::Path, screen_w: u32, screen_h: u32, mode: &str) -> Result<RgbImage> {
    let img = image::open(path)?;
    process_image(img, screen_w, screen_h, mode)
}

/// Process image from downloaded byte slice.
pub fn process_bytes(bytes: &[u8], screen_w: u32, screen_h: u32, mode: &str) -> Result<RgbImage> {
    let img = image::load_from_memory(bytes).or_else(|_| {
        let cursor = Cursor::new(bytes);
        let reader = image::ImageReader::new(cursor)
            .with_guessed_format()
            .map_err(image::ImageError::IoError)?;
        reader.decode()
    })?;
    process_image(img, screen_w, screen_h, mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_fit_scales_and_centers() {
        assert_eq!(cover_fit(1920, 1080, 1080, 2400), (4267, 2400, 1593, 0));
        assert_eq!(cover_fit(1080, 2400, 1920, 1080), (1920, 4267, 0, 1593));
        assert_eq!(cover_fit(1000, 1000, 1080, 2400), (2400, 2400, 660, 0));
    }

    #[test]
    fn smart_cover_fit_protects_top() {
        // When excess vertical height exists, top crop is 35% instead of 50%
        let (_, _, crop_x, crop_y) = smart_cover_fit(1000, 3000, 1000, 2000);
        assert_eq!(crop_x, 0);
        assert_eq!(crop_y, 350); // 1000 excess * 0.35 = 350 (vs 500 in symmetrical)
    }

    #[test]
    fn contain_blur_mode_works() {
        let test_img = image::DynamicImage::ImageRgb8(RgbImage::new(100, 200));
        let res = process_image(test_img, 1080, 2400, "contain_blur");
        assert!(res.is_ok());
        let img = res.unwrap();
        assert_eq!(img.dimensions(), (1080, 2400));
    }
}

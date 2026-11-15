# wall

Fast, intelligent wallpaper setter for Android via Termux. Powered by [WallWidgy](https://www.wallwidgy.app) and local storage. Sets the exact same image on both home and lock screen.

## Highlights

- **Uncapped High-Res Downloads**: Full 4K uncompressed wallpapers streamed at maximum network speed.
- **Smart Cropping**: Intelligently centers horizontally while preserving the top 35% vertically so characters, faces, skies, and horizons are never cut off.
- **Fit Modes**: Choose between `smart`, `cover` (symmetrical center), and `contain_blur` (0% cropped, ambient blurred backdrop).
- **Auto-Config Setup**: Automatically generates a clean, well-commented configuration on first launch — no interactive prompts blocking headless crons.
- **Filters & Search**: Pick by category (`-c`), filter by color (`-C`), or search by title/tag (`-s`).
- **Priority & Blacklist**: Bias random mode towards your favorite aesthetics and block unwanted categories/colors.

## Requirements

- [Termux](https://f-droid.org/packages/com.termux/)
- [Termux:API](https://f-droid.org/packages/com.termux.api/) Android app installed
- `termux-api` package: `pkg install termux-api`

## Install

1. Download `wall` from [Releases](../../releases/latest)
2. Move it to your Termux bin folder and make it executable:

```sh
cp /sdcard/Download/wall ~/bin/wall
chmod +x ~/bin/wall
```

3. Run once. It will detect your screen resolution and auto-create `~/.config/wall/config.toml`:

```sh
wall
```

## Usage

```sh
# Random wallpaper (respects priority categories & preferred colors)
wall

# Specific category (e.g. anime, minimal, amoled, art, nature)
wall -c anime
wall -c amoled

# Filter by color (e.g. blue, red, orange, black)
wall -C blue

# Combine category + color
wall -c art -C blue

# Search by title or keyword (first mobile match is used)
wall -s "cat"
wall -s "cyberpunk skyline"

# List categories and colors with your priority and blacklist status
wall -l

# List official categories and supported colors
wall --categories
wall --colors

# View active configuration and screen resolution
wall --info

# Reset config.toml to the default template
wall --reset-config

# Test without applying to wallpaper (great for testing)
wall --dry-run -c amoled

# Fall back to local wallpaper folder
wall --local
```

## Configuration

Configuration lives at `~/.config/wall/config.toml`:

```toml
[screen]
# Screen dimensions for your Android device in pixels
width = 1080
height = 2400

[fit]
# Cropping and scaling algorithm:
# - "smart"       : Intelligent cover-fit (centers horizontally, protects top 35% vertically).
# - "cover"       : Standard symmetrical center-crop.
# - "contain_blur": Fits entire wallpaper without any cropping;
#                   fills letterbox bars with a smooth, dimmed blurred ambient backdrop.
mode = "smart"

[filter]
# Categories to exclude (case-insensitive)
blacklist = ["cars"]

# Preferred categories for random mode (60% chance to pick from these)
priority = ["minimal", "amoled", "art"]

# Preferred colors for random mode (e.g. "blue", "purple", "black")
colors = ["blue"]

# Colors to exclude completely
blacklist_colors = []

[source]
# Optional fallback folder when running `wall --local`
# walls_dir = "/sdcard/Pictures/Wallpapers"
```

### Resetting Configuration

To restore the default configuration template at any time:

```sh
wall --reset-config
```

## Scheduling

To rotate wallpapers automatically in the background:

```sh
crontab -e
# Rotate every hour
0 * * * * ~/bin/wall
```

## How It Works

- **WallWidgy API**: Direct REST API integration with `type=mobile` hardcoded for phone screen aspect ratios.
- **Lanczos3 Resampling**: Downscales and upscales using convolution resampling with `fast_image_resize` for crisp detail.
- **Dual Screen Sync**: Writes one high-res PNG to `~/.cache/wall/wall_tmp.png` and sets both home screen (`termux-wallpaper -f`) and lock screen (`termux-wallpaper -f -l`).

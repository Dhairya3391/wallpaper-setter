# Wallpaper Setter

A Rust-based wallpaper management tool designed for Android devices running Termux. This tool automatically selects and sets wallpapers from your collection, ensuring you never see the same wallpaper twice until all wallpapers have been used.

## Features

- **Smart Wallpaper Rotation**: Automatically selects wallpapers from your collection without repeating until all have been used
- **Intelligent Image Processing**: Resizes and crops images to perfectly fit your device's screen resolution (1080x2400)
- **Aspect Ratio Handling**: Intelligently handles different image aspect ratios including ultra-wide and ultra-tall images
- **Dual Screen Support**: Sets wallpapers for both home screen and lock screen
- **Persistent Configuration**: Remembers used wallpapers and favorites across sessions
- **Multiple Format Support**: Supports PNG, JPG, JPEG, and WebP image formats

## Prerequisites

- **Termux**: This tool is designed to run on Android devices using Termux
- **Rust**: Rust toolchain must be installed in Termux
- **Termux API**: Required for setting wallpapers

## Installation

1. **Install Rust in Termux**:

   ```bash
   pkg install rust
   ```

2. **Install Termux API** (for wallpaper setting functionality):

   ```bash
   pkg install termux-api
   ```

3. **Clone or download the project**:

   ```bash
   git clone <repository-url>
   cd wallpaper-setter
   ```

4. **Build the project**:
   ```bash
   cargo build --release
   ```

## Setup

1. **Create your wallpaper directory**:

   ```bash
   mkdir -p ~/storage/shared/.Walls
   ```

2. **Add your wallpapers**: Place your wallpaper images (PNG, JPG, JPEG, or WebP) in the `~/storage/shared/.Walls` directory.

3. **Grant storage permissions** (if needed):
   ```bash
   termux-setup-storage
   ```

## Usage

### Basic Usage

Simply run the compiled binary to set a random wallpaper:

```bash
./target/release/wall
```

### Automation

You can automate wallpaper changes by:

1. **Setting up a cron job** (if available in your Termux setup)
2. **Using Tasker** or similar automation apps
3. **Creating a simple script** that runs the tool periodically

Example script for changing wallpaper every hour:

```bash
#!/bin/bash
while true; do
    ./target/release/wall
    sleep 3600  # Wait 1 hour
done
```

## Configuration

The tool automatically creates a configuration file at `~/.config/wallpaper_manager.json` that tracks:

- **Used wallpapers**: Prevents repetition until all wallpapers have been used
- **Favorites**: (Future feature) Mark your favorite wallpapers
- **Current wallpaper**: Tracks the currently set wallpaper

## Technical Details

### Image Processing

The tool uses advanced image processing to ensure optimal wallpaper display:

- **Smart Resizing**: Images are resized to fit your device's 1080x2400 resolution
- **Aspect Ratio Preservation**: Maintains image quality while ensuring full screen coverage
- **Center Cropping**: For images that don't match the device aspect ratio, the tool intelligently crops from the center
- **Ultra-wide/Tall Handling**: Special handling for images with extreme aspect ratios

### Supported Image Formats

- PNG
- JPG/JPEG
- WebP

### Device Compatibility

Currently configured for devices with 1080x2400 resolution. To modify for different screen sizes, edit the constants in `main.rs`:

```rust
const DEVICE_WIDTH: u32 = 1080;
const DEVICE_HEIGHT: u32 = 2400;
```

## Dependencies

- **rand**: Random number generation for wallpaper selection
- **image**: Image processing and manipulation
- **serde**: Serialization for configuration management
- **fast_image_resize**: High-performance image resizing
- **walkdir**: Directory traversal for finding wallpapers
- **dirs**: Cross-platform directory handling
- **serde_json**: JSON serialization

## Building

### Development Build

```bash
cargo build
```

### Release Build (Optimized)

```bash
cargo build --release
```

The release build includes optimizations like Link Time Optimization (LTO) and reduced codegen units for better performance.

## Troubleshooting

### Common Issues

1. **"No wallpapers found"**: Ensure you have images in `~/storage/shared/.Walls`
2. **Permission denied**: Make sure Termux has storage permissions
3. **Wallpaper not setting**: Verify Termux API is installed and working

### Debug Mode

For debugging, you can run with verbose output by modifying the code to add more print statements.

## Contributing

Feel free to contribute to this project by:

1. Reporting bugs
2. Suggesting new features
3. Submitting pull requests
4. Improving documentation

## License

[Add your license information here]

## Acknowledgments

- Built with Rust for performance and reliability
- Uses Termux for Android compatibility
- Leverages fast_image_resize for efficient image processing

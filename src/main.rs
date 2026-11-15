//! wall — fast, intelligent wallpaper setter for Android via Termux.
//! Fetches mobile wallpapers from WallWidgy with smart cropping, color/category filtering,
//! blacklist support, and prioritized random selection.

mod api;
mod config;
mod image_proc;
mod setter;
mod update;

use api::WallpaperEntry;
use clap::Parser;
use config::Config;
use rand::seq::SliceRandom;
use std::process::ExitCode;

// ── CLI ──────────────────────────────────────────────────────────────────────

/// Intelligent Android wallpaper setter powered by WallWidgy.
#[derive(Parser)]
#[command(
    name = "wall",
    version,
    about = "Fast, smart wallpaper setter for Android via Termux"
)]
struct Cli {
    /// Select wallpaper from a specific category (e.g. anime, minimal, amoled, art).
    #[arg(short = 'c', long = "category", value_name = "NAME")]
    category: Option<String>,

    /// Filter wallpapers by color (e.g. blue, purple, black, neon). Combinable with -c.
    #[arg(short = 'C', long = "color", value_name = "COLOR")]
    color: Option<String>,

    /// Search for a wallpaper by keyword or title (first match is selected).
    #[arg(short = 's', long = "search", value_name = "QUERY")]
    search: Option<String>,

    /// List all categories and colors with priority and blacklist status.
    #[arg(short = 'l', long = "list")]
    list: bool,

    /// List official WallWidgy categories.
    #[arg(long = "categories")]
    categories: bool,

    /// List all supported color filters.
    #[arg(long = "colors")]
    colors: bool,

    /// Reset configuration file to the clean default template.
    #[arg(long = "reset-config")]
    reset_config: bool,

    /// Print configuration file path.
    #[arg(long = "config")]
    show_config: bool,

    /// Display current active settings (screen size, fit mode, priorities, blacklist).
    #[arg(long = "info")]
    info: bool,

    /// Update wall to the latest release directly from GitHub in-place.
    #[arg(short = 'u', long = "update")]
    update: bool,

    /// Use local wallpaper folder instead of online WallWidgy source.
    #[arg(long = "local")]
    local: bool,

    /// Number of wallpaper candidates to fetch for selection pool (default: 30).
    #[arg(short = 'n', default_value = "30")]
    count: u32,

    /// Download and process image without actually applying to home/lock screen.
    #[arg(long = "dry-run")]
    dry_run: bool,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

fn run_category(
    cat: &str,
    color: Option<&str>,
    cfg: &Config,
    count: u32,
    dry_run: bool,
) -> Result<(), String> {
    if cfg.is_blacklisted(cat) {
        return Err(format!("Category '{cat}' is blacklisted in your config."));
    }
    if let Some(c) = color {
        if cfg.is_color_blacklisted(c) {
            return Err(format!("Color '{c}' is blacklisted in your config."));
        }
    }

    eprint!("Fetching wallpapers from category: {cat}");
    if let Some(c) = color {
        eprint!(", color: {c}");
    }
    eprintln!("...");

    let walls = api::query(Some(cat), color, count)?;
    if walls.is_empty() {
        if let Some(c) = color {
            return Err(format!("No wallpapers found for category '{cat}' with color '{c}'"));
        } else {
            return Err(format!("No wallpapers found for category '{cat}'"));
        }
    }

    let wall = walls.choose(&mut rand::thread_rng()).ok_or("Empty pool")?;
    download_process_set(wall, cfg, dry_run)
}

fn run_color(color: &str, cfg: &Config, count: u32, dry_run: bool) -> Result<(), String> {
    if cfg.is_color_blacklisted(color) {
        return Err(format!("Color '{color}' is blacklisted in your config."));
    }

    eprintln!("Fetching wallpapers with color: {color}...");
    let mut walls = api::query(None, Some(color), count)?;
    walls.retain(|w| {
        if let Some(c) = w.category() {
            !cfg.is_blacklisted(&c)
        } else {
            true
        }
    });

    if walls.is_empty() {
        return Err(format!("No wallpapers found for color '{color}'"));
    }

    let wall = walls.choose(&mut rand::thread_rng()).ok_or("Empty pool")?;
    download_process_set(wall, cfg, dry_run)
}

fn run_search(
    query: &str,
    color: Option<&str>,
    cfg: &Config,
    count: u32,
    dry_run: bool,
) -> Result<(), String> {
    eprintln!("Searching WallWidgy for: \"{query}\"...");
    let mut walls = api::search(query, count)?;
    walls.retain(|w| {
        let cat_ok = w.category().map(|c| !cfg.is_blacklisted(&c)).unwrap_or(true);
        let col_bl_ok = !w.has_blacklisted_color(&cfg.filter.blacklist_colors);
        let col_match_ok = if let Some(c) = color {
            w.matches_color(c)
        } else {
            true
        };
        cat_ok && col_bl_ok && col_match_ok
    });

    if walls.is_empty() {
        if let Some(c) = color {
            return Err(format!("No wallpapers matched query \"{query}\" with color \"{c}\""));
        }
        return Err(format!("No wallpapers matched query: \"{query}\""));
    }

    // First match as requested
    let wall = &walls[0];
    download_process_set(wall, cfg, dry_run)
}

fn run_random(cfg: &Config, count: u32, dry_run: bool) -> Result<(), String> {
    eprintln!("Fetching random wallpaper...");

    let mut rng = rand::thread_rng();
    let has_prio = !cfg.filter.priority.is_empty();
    let has_colors = !cfg.filter.colors.is_empty();

    // 60% bias towards user's priority categories or preferred colors
    if has_prio || has_colors {
        let roll: f32 = rand::random();
        if roll < 0.60 {
            let cat = if has_prio {
                cfg.filter.priority.choose(&mut rng).map(|s| s.as_str())
            } else {
                None
            };
            let col = if has_colors {
                cfg.filter.colors.choose(&mut rng).map(|s| s.as_str())
            } else {
                None
            };

            if cat.is_some() || col.is_some() {
                if let Some(c) = cat {
                    eprint!("  ★ Prioritizing category: {c}");
                }
                if let Some(c) = col {
                    eprint!("  ★ Preferred color: {c}");
                }
                eprintln!();

                if let Ok(walls) = api::query(cat, col, count) {
                    let filtered: Vec<_> = walls
                        .into_iter()
                        .filter(|w| {
                            w.category().map(|c| !cfg.is_blacklisted(&c)).unwrap_or(true)
                        })
                        .collect();
                    if let Some(wall) = filtered.choose(&mut rng) {
                        return download_process_set(wall, cfg, dry_run);
                    }
                }
            }
        }
    }

    // General random pool
    let walls = api::random_many(count)?;
    let filtered: Vec<_> = walls
        .into_iter()
        .filter(|w| {
            w.category().map(|c| !cfg.is_blacklisted(&c)).unwrap_or(true)
        })
        .collect();

    if let Some(wall) = filtered.choose(&mut rng) {
        return download_process_set(wall, cfg, dry_run);
    }

    Err("Could not retrieve a valid random wallpaper. Please try again.".into())
}

fn run_local(cfg: &Config, dry_run: bool) -> Result<(), String> {
    let dir = cfg
        .source
        .walls_dir
        .as_deref()
        .ok_or("No walls_dir configured. Add `walls_dir = \"/path/to/folder\"` in ~/.config/wall/config.toml")?;

    let dir = std::path::Path::new(dir);
    if !dir.is_dir() {
        return Err(format!("Local directory not found: {}", dir.display()));
    }

    let walls = local_wallpapers(dir);
    if walls.is_empty() {
        return Err(format!("No supported images found in {}", dir.display()));
    }

    let path = walls.choose(&mut rand::thread_rng()).unwrap();
    eprintln!("Local: {}", path.display());

    let image = image_proc::process_file(path, cfg.screen.width, cfg.screen.height, &cfg.fit.mode)
        .map_err(|e| format!("Image processing failed: {e}"))?;

    if dry_run {
        println!("Dry-run complete (image processed).");
        return Ok(());
    }

    setter::set_wallpaper(&image, &config::cache_dir())?;
    println!("OK  {}", path.file_name().unwrap_or_default().to_string_lossy());
    Ok(())
}

fn run_list(cfg: &Config) {
    println!("┌──────────────────────────────────────────────────┐");
    println!("│ Official WallWidgy Categories                    │");
    println!("└──────────────────────────────────────────────────┘");
    for cat in api::OFFICIAL_CATEGORIES {
        let is_prio = cfg.filter.priority.iter().any(|p| p.eq_ignore_ascii_case(cat));
        let is_bl = cfg.is_blacklisted(cat);
        let badge = if is_bl {
            " [BLACKLISTED]"
        } else if is_prio {
            " ★ [PRIORITY]"
        } else {
            ""
        };
        println!("  • {cat}{badge}");
    }

    println!("\n┌──────────────────────────────────────────────────┐");
    println!("│ Color Palette                                    │");
    println!("└──────────────────────────────────────────────────┘");
    let colors = api::available_colors();
    for col in &colors {
        let is_pref = cfg.filter.colors.iter().any(|c| c.eq_ignore_ascii_case(col));
        let is_bl = cfg.is_color_blacklisted(col);
        let badge = if is_bl {
            " [BLACKLISTED]"
        } else if is_pref {
            " ★ [PREFERRED]"
        } else {
            ""
        };
        println!("  • {col}{badge}");
    }
}

fn run_info(cfg: &Config) {
    println!("wall configuration info:");
    println!("  Config path : {}", config::config_path().display());
    println!("  Resolution  : {}x{} px", cfg.screen.width, cfg.screen.height);
    println!("  Fit mode    : {}", cfg.fit.mode);
    println!("  Blacklist   : {:?}", cfg.filter.blacklist);
    println!("  Priority    : {:?}", cfg.filter.priority);
    println!("  Colors      : {:?}", cfg.filter.colors);
    println!("  Blacklist C : {:?}", cfg.filter.blacklist_colors);
    if let Some(ref dir) = cfg.source.walls_dir {
        println!("  Local dir   : {dir}");
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn download_process_set(wall: &WallpaperEntry, cfg: &Config, dry_run: bool) -> Result<(), String> {
    let url = wall
        .download_url()
        .ok_or("Wallpaper does not have a valid download URL")?;

    let title = wall.title();
    eprintln!("Downloading: \"{title}\"");
    if let Some(cat) = wall.category() {
        eprintln!("  Category   : {cat}");
    }
    if let Some(res) = wall.resolution() {
        eprintln!("  Resolution : {res}");
    }
    if let Some(mood) = wall.mood() {
        eprintln!("  Mood       : {mood}");
    }
    let colors = wall.primary_colors();
    if !colors.is_empty() {
        eprintln!("  Colors     : {}", colors.join(", "));
    }
    let tags = wall.tags();
    if !tags.is_empty() {
        let preview = tags.iter().take(5).cloned().collect::<Vec<_>>().join(", ");
        eprintln!("  Tags       : {preview}");
    }
    if let Some(desc) = wall.description() {
        let short = if desc.len() > 80 {
            format!("{}...", &desc[..80])
        } else {
            desc.to_string()
        };
        eprintln!("  About      : {short}");
    }

    let bytes = api::download_image(&url)?;
    let size_mb = bytes.len() as f64 / (1024.0 * 1024.0);
    eprintln!("  Size       : {size_mb:.2} MB");

    let image = image_proc::process_bytes(&bytes, cfg.screen.width, cfg.screen.height, &cfg.fit.mode)
        .map_err(|e| format!("Processing failed: {e}"))?;

    if dry_run {
        println!("Dry run complete. Wallpaper processed successfully (fit: {}).", cfg.fit.mode);
        return Ok(());
    }

    setter::set_wallpaper(&image, &config::cache_dir())?;
    println!("OK  {title}");
    Ok(())
}

const SUPPORTED_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

fn local_wallpapers(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    fn is_supported(path: &std::path::Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| SUPPORTED_EXTENSIONS.iter().any(|&s| ext.eq_ignore_ascii_case(s)))
    }

    fn collect(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.is_file() && is_supported(&path) {
                out.push(path);
            }
        }
    }

    let mut out = Vec::new();
    collect(dir, &mut out);
    out
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Fast exits that don't need or can overwrite config
    if cli.show_config {
        println!("{}", config::config_path().display());
        return ExitCode::SUCCESS;
    }

    if cli.update {
        return match update::update() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Update error: {e}");
                ExitCode::FAILURE
            }
        };
    }

    if cli.reset_config {
        match Config::reset_to_default() {
            Ok(_) => {
                println!("Configuration reset to default template at: {}", config::config_path().display());
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("Failed to reset config: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    if cli.categories {
        println!("Official WallWidgy categories:");
        for c in api::OFFICIAL_CATEGORIES {
            println!("  • {c}");
        }
        return ExitCode::SUCCESS;
    }

    if cli.colors {
        println!("Supported WallWidgy colors:");
        for c in api::available_colors() {
            println!("  • {c}");
        }
        return ExitCode::SUCCESS;
    }

    // Load or automatically initialize configuration on first run
    let (cfg, is_new) = match Config::load_or_create() {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Configuration error: {e}");
            return ExitCode::FAILURE;
        }
    };

    if is_new {
        eprintln!(
            "✨ Initialized default configuration at {} ({}x{} px)",
            config::config_path().display(),
            cfg.screen.width,
            cfg.screen.height
        );
    }

    if cli.info {
        run_info(&cfg);
        return ExitCode::SUCCESS;
    }

    if cli.list {
        run_list(&cfg);
        return ExitCode::SUCCESS;
    }

    let result = if let Some(ref query) = cli.search {
        run_search(query, cli.color.as_deref(), &cfg, cli.count, cli.dry_run)
    } else if cli.local {
        run_local(&cfg, cli.dry_run)
    } else {
        match (cli.category.as_deref(), cli.color.as_deref()) {
            (Some(cat), color) => run_category(cat, color, &cfg, cli.count, cli.dry_run),
            (None, Some(color)) => run_color(color, &cfg, cli.count, cli.dry_run),
            (None, None) => run_random(&cfg, cli.count, cli.dry_run),
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

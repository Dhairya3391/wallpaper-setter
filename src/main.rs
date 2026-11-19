//! wall — fast, intelligent wallpaper setter for Android via Termux.
//! Fetches mobile wallpapers from WallWidgy with smart cropping, color/category filtering,
//! blacklist support, and prioritized random selection.

mod api;
mod config;
mod history;
mod image_proc;
mod setter;
mod update;

use api::{Filters, WallpaperEntry};
use clap::{Parser, ValueEnum};
use config::Config;
use rand::seq::SliceRandom;
use rand::Rng;
use std::process::ExitCode;

// ── CLI ──────────────────────────────────────────────────────────────────────

/// How to combine multiple color filters.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum MatchMode {
    /// Any listed color may match (OR).
    #[value(alias = "or")]
    Any,
    /// Every listed color must match (AND).
    #[value(alias = "and")]
    All,
}

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

    /// Filter by one or more colors, comma-separated or repeatable (e.g. -C black,white).
    #[arg(short = 'C', long = "color", num_args(1..), value_name = "COLOR")]
    color_values: Vec<String>,

    /// Filter by one or more moods, comma-separated or repeatable (e.g. -m calm,dreamy).
    #[arg(short = 'm', long = "mood", num_args(1..), value_name = "MOOD")]
    mood_values: Vec<String>,

    /// Combine multiple colors with any (OR) or all (AND). mood always uses any.
    #[arg(long = "match", value_enum, default_value = "any")]
    match_mode: MatchMode,

    /// Search for a wallpaper by keyword or title (an unseen match is selected).
    #[arg(short = 's', long = "search", num_args(1..), value_name = "QUERY")]
    search: Option<Vec<String>>,

    /// List all categories and colors with priority and blacklist status.
    #[arg(short = 'l', long = "list")]
    list: bool,

    /// List official WallWidgy categories.
    #[arg(long = "categories")]
    categories: bool,

    /// List all supported color filters.
    #[arg(long = "colors")]
    colors: bool,

    /// List all supported mood filters.
    #[arg(long = "moods")]
    moods: bool,

    /// Reset configuration file to the clean default template.
    #[arg(long = "reset-config")]
    reset_config: bool,

    /// Clear the list of wallpapers that have already been applied.
    #[arg(long = "reset-history")]
    reset_history: bool,

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
    filters: Filters<'_>,
    cfg: &Config,
    count: u32,
    dry_run: bool,
    history: &mut history::History,
) -> Result<(), String> {
    if cfg.is_blacklisted(cat) {
        return Err(format!("Category '{cat}' is blacklisted in your config."));
    }
    check_color_blacklist(filters.colors, cfg)?;

    eprint!("Fetching wallpapers from category: {cat}");
    if !filters.colors.is_empty() {
        eprint!(", color: {}", filters.colors.join(", "));
    }
    if !filters.moods.is_empty() {
        eprint!(", mood: {}", filters.moods.join(", "));
    }
    eprintln!("...");

    let mut walls = api::discover(Some(cat), filters, count)?;
    walls.retain(|w| !w.has_blacklisted_color(&cfg.filter.blacklist_colors));
    if walls.is_empty() {
        return Err(format!("No wallpapers found for category '{cat}' with the requested filters."));
    }

    let wall = pick_unseen(walls, history, &mut rand::thread_rng()).ok_or("Empty pool")?;
    download_process_set(&wall, cfg, dry_run, history)
}

fn run_color(
    filters: Filters<'_>,
    cfg: &Config,
    count: u32,
    dry_run: bool,
    history: &mut history::History,
) -> Result<(), String> {
    check_color_blacklist(filters.colors, cfg)?;

    if filters.colors.is_empty() {
        eprintln!("Fetching wallpapers with mood: {}...", filters.moods.join(", "));
    } else {
        eprint!("Fetching wallpapers with color: {}", filters.colors.join(", "));
        if filters.match_all {
            eprint!(" (all)");
        }
        eprintln!("...");
        if !filters.moods.is_empty() {
            eprintln!("  Mood: {}", filters.moods.join(", "));
        }
    }

    let mut walls = api::discover(None, filters, count)?;
    walls.retain(|w| {
        let cat_ok = w.category().map(|c| !cfg.is_blacklisted(&c)).unwrap_or(true);
        cat_ok && !w.has_blacklisted_color(&cfg.filter.blacklist_colors)
    });

    if walls.is_empty() {
        return Err("No wallpapers found matching the requested filters.".into());
    }

    let wall = pick_unseen(walls, history, &mut rand::thread_rng()).ok_or("Empty pool")?;
    download_process_set(&wall, cfg, dry_run, history)
}

fn run_search(
    query: &str,
    filters: Filters<'_>,
    cfg: &Config,
    count: u32,
    dry_run: bool,
    history: &mut history::History,
) -> Result<(), String> {
    check_color_blacklist(filters.colors, cfg)?;

    eprintln!("Searching WallWidgy for: \"{query}\"...");
    let mut walls = api::search(query, count)?;
    walls.retain(|w| {
        let cat_ok = w.category().map(|c| !cfg.is_blacklisted(&c)).unwrap_or(true);
        let col_bl_ok = !w.has_blacklisted_color(&cfg.filter.blacklist_colors);
        cat_ok && col_bl_ok && filters.matches(w, true)
    });

    if walls.is_empty() {
        return Err(format!("No wallpapers matched query \"{query}\" with the requested filters."));
    }

    let wall = pick_unseen(walls, history, &mut rand::thread_rng()).ok_or("Empty pool")?;
    download_process_set(&wall, cfg, dry_run, history)
}

fn run_random(
    cfg: &Config,
    count: u32,
    dry_run: bool,
    history: &mut history::History,
) -> Result<(), String> {
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

                let col_values: Vec<String> = col.map(|c| vec![c.to_string()]).unwrap_or_default();
                let filters = Filters {
                    colors: &col_values,
                    moods: &[],
                    match_all: false,
                };
                if let Ok(walls) = api::discover(cat, filters, count) {
                    let filtered: Vec<_> = walls
                        .into_iter()
                        .filter(|w| {
                            w.category().map(|c| !cfg.is_blacklisted(&c)).unwrap_or(true)
                        })
                        .collect();
                    if let Some(wall) = pick_unseen(filtered, history, &mut rng) {
                        return download_process_set(&wall, cfg, dry_run, history);
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

    if let Some(wall) = pick_unseen(filtered, history, &mut rng) {
        return download_process_set(&wall, cfg, dry_run, history);
    }

    Err("Could not retrieve a valid random wallpaper. Please try again.".into())
}

fn run_local(cfg: &Config, dry_run: bool, history: &mut history::History) -> Result<(), String> {
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

    let mut rng = rand::thread_rng();
    let unseen: Vec<&std::path::PathBuf> = walls
        .iter()
        .filter(|p| !history.contains(&local_id(p)))
        .collect();
    let chosen = if let Some(p) = unseen.choose(&mut rng) {
        Some(*p)
    } else {
        let last = history.last().unwrap_or("");
        let recycled: Vec<&std::path::PathBuf> = walls
            .iter()
            .filter(|p| local_id(p) != last)
            .collect();
        if recycled.is_empty() {
            eprintln!("  ↻ No unseen local wallpapers; reapplying the only candidate.");
            walls.first()
        } else {
            eprintln!("  ↻ All local wallpapers already applied; recycling older ones.");
            recycled.choose(&mut rng).copied()
        }
    };

    let path = chosen.ok_or("Empty pool")?;
    eprintln!("Local: {}", path.display());

    let image = image_proc::process_file(path, cfg.screen.width, cfg.screen.height, &cfg.fit.mode)
        .map_err(|e| format!("Image processing failed: {e}"))?;

    if dry_run {
        println!("Dry-run complete (image processed).");
        return Ok(());
    }

    setter::set_wallpaper(&image, &config::cache_dir())?;
    history.record(&local_id(path));
    if let Err(e) = history.save() {
        eprintln!("  Warning: could not save wallpaper history: {e}");
    }
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

fn run_info(cfg: &Config, history: &history::History) {
    println!("wall configuration info:");
    println!("  Config path : {}", config::config_path().display());
    println!("  Resolution  : {}x{} px", cfg.screen.width, cfg.screen.height);
    println!("  Fit mode    : {}", cfg.fit.mode);
    println!("  Blacklist   : {:?}", cfg.filter.blacklist);
    println!("  Priority    : {:?}", cfg.filter.priority);
    println!("  Colors      : {:?}", cfg.filter.colors);
    println!("  Blacklist C : {:?}", cfg.filter.blacklist_colors);
    println!(
        "  Applied     : {} wallpaper(s) in {}",
        history.len(),
        history.path().display()
    );
    if let Some(ref dir) = cfg.source.walls_dir {
        println!("  Local dir   : {dir}");
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Split comma-separated CLI values (also accepts repeated flags) into a clean,
/// deduplicated, lowercase list.
fn parse_multi(values: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        for part in value.split(',') {
            let part = part.trim().to_lowercase();
            if !part.is_empty() && !out.contains(&part) {
                out.push(part);
            }
        }
    }
    out
}

fn check_color_blacklist(colors: &[String], cfg: &Config) -> Result<(), String> {
    for c in colors {
        if cfg.is_color_blacklisted(c) {
            return Err(format!("Color '{c}' is blacklisted in your config."));
        }
    }
    Ok(())
}

/// Pick a wallpaper that has not been applied yet. Once the whole pool has been
/// seen, recycle it but exclude the single most recently applied wallpaper so
/// the same image never appears twice in a row.
fn pick_unseen<R: Rng>(
    walls: Vec<WallpaperEntry>,
    history: &history::History,
    rng: &mut R,
) -> Option<WallpaperEntry> {
    if walls.is_empty() {
        return None;
    }

    let unseen: Vec<WallpaperEntry> = walls
        .iter()
        .filter(|w| !history.contains(&w.id()))
        .cloned()
        .collect();
    if let Some(pick) = unseen.choose(rng) {
        return Some(pick.clone());
    }

    let last = history.last().unwrap_or("");
    let first = walls.first().cloned();
    let recycled: Vec<WallpaperEntry> = walls
        .into_iter()
        .filter(|w| w.id() != last)
        .collect();
    if recycled.is_empty() {
        eprintln!("  ↻ No unseen wallpapers in this pool; reapplying the only candidate.");
        return first;
    }
    eprintln!("  ↻ All candidates already applied; recycling older wallpapers.");
    recycled.choose(rng).cloned()
}

/// Stable key for a local wallpaper file.
fn local_id(path: &std::path::Path) -> String {
    path.to_string_lossy().trim().to_lowercase()
}

fn download_process_set(
    wall: &WallpaperEntry,
    cfg: &Config,
    dry_run: bool,
    history: &mut history::History,
) -> Result<(), String> {
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

    history.record(&wall.id());
    if let Err(e) = history.save() {
        eprintln!("  Warning: could not save wallpaper history: {e}");
    }
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

    if cli.reset_history {
        let mut h = history::History::load();
        return match h.clear() {
            Ok(()) => {
                println!(
                    "Applied wallpaper history cleared ({})",
                    config::history_path().display()
                );
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("Failed to reset history: {e}");
                ExitCode::FAILURE
            }
        };
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

    if cli.moods {
        println!("Supported WallWidgy moods:");
        for m in api::MOODS {
            println!("  • {m}");
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

    let mut history = history::History::load();

    if cli.info {
        run_info(&cfg, &history);
        return ExitCode::SUCCESS;
    }

    if cli.list {
        run_list(&cfg);
        return ExitCode::SUCCESS;
    }

    let search_query = cli.search.map(|words| words.join(" "));
    let colors = parse_multi(&cli.color_values);
    let moods = parse_multi(&cli.mood_values);
    let filters = Filters {
        colors: &colors,
        moods: &moods,
        match_all: matches!(cli.match_mode, MatchMode::All),
    };

    let result = if let Some(ref query) = search_query {
        run_search(query, filters, &cfg, cli.count, cli.dry_run, &mut history)
    } else if cli.local {
        run_local(&cfg, cli.dry_run, &mut history)
    } else if let Some(cat) = cli.category.as_deref() {
        run_category(cat, filters, &cfg, cli.count, cli.dry_run, &mut history)
    } else if !colors.is_empty() || !moods.is_empty() {
        run_color(filters, &cfg, cli.count, cli.dry_run, &mut history)
    } else {
        run_random(&cfg, cli.count, cli.dry_run, &mut history)
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_multi_handles_commas_spaces_and_repeats() {
        let values = vec![
            "black, white".to_string(),
            "blue,".to_string(),
            String::new(),
            "BLACK".to_string(),
        ];
        assert_eq!(parse_multi(&values), vec!["black", "white", "blue"]);
    }
}

//! WallWidgy API client — high-speed, no artificial download caps.
//! Always requests mobile-oriented wallpapers for Android/Termux.

use serde::Deserialize;
use std::io::Read;
use std::time::Duration;

const BASE: &str = "https://wallwidgy.app/api";
const STORAGE_RAW: &str = "https://raw.githubusercontent.com/not-ayan/storage/main/main";
const DEVICE: &str = "mobile";
const TIMEOUT: Duration = Duration::from_secs(25);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Official WallWidgy categories.
pub const OFFICIAL_CATEGORIES: &[&str] = &[
    "abstract",
    "amoled",
    "anime",
    "architecture",
    "art",
    "cars",
    "minimal",
    "nature",
    "tech",
];

/// Official WallWidgy color palette.
pub const OFFICIAL_COLORS: &[&str] = &[
    "amber", "azure", "beige", "black", "blue", "brown", "burgundy",
    "charcoal", "coral", "cream", "crimson", "cyan", "dark-blue", "dark-green",
    "emerald", "forest", "fuchsia", "gold", "gray", "green", "lavender",
    "lime", "magenta", "maroon", "navy", "olive", "orange", "peach",
    "pink", "purple", "red", "rose", "scarlet", "silver", "tan", "teal",
    "violet", "white", "yellow",
];

/// Unified wallpaper entry that handles both string URLs and rich metadata objects.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum WallpaperEntry {
    /// Direct image URL string from `/api/wallpapers`.
    Url(String),
    /// Structured wallpaper record from `/api/random-wallpapers` or `/api/wallpapers/search`.
    Object {
        #[serde(default, alias = "public_id")]
        name: Option<String>,
        #[serde(default)]
        file_name: Option<String>,
        #[serde(default)]
        file_main_name: Option<String>,
        #[serde(default, alias = "url")]
        download_url: Option<String>,
        #[serde(default)]
        width: Option<u32>,
        #[serde(default)]
        height: Option<u32>,
        #[serde(default)]
        resolution: Option<String>,
        #[serde(default)]
        category: Option<String>,
        #[serde(default)]
        orientation: Option<String>,
        #[serde(default)]
        data: Option<WallpaperData>,
    },
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct WallpaperData {
    #[serde(default)]
    pub primary_colors: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub scene_description: Option<String>,
    #[serde(default)]
    pub mood: Option<String>,
}

impl WallpaperEntry {
    /// Extracts the direct, uncompressed image download URL.
    pub fn download_url(&self) -> Option<String> {
        match self {
            WallpaperEntry::Url(u) => Some(u.clone()),
            WallpaperEntry::Object {
                download_url,
                file_main_name,
                file_name,
                ..
            } => {
                if let Some(u) = download_url {
                    if !u.is_empty() {
                        return Some(u.clone());
                    }
                }
                if let Some(f) = file_main_name {
                    if !f.is_empty() {
                        return Some(format!("{STORAGE_RAW}/{f}"));
                    }
                }
                if let Some(f) = file_name {
                    if !f.is_empty() {
                        let filename = if f.contains('.') { f.clone() } else { format!("{f}.png") };
                        return Some(format!("{STORAGE_RAW}/{filename}"));
                    }
                }
                None
            }
        }
    }

    /// User-friendly title formatted cleanly.
    pub fn title(&self) -> String {
        match self {
            WallpaperEntry::Url(u) => {
                let file = u.rsplit('/').next().unwrap_or(u);
                let name = file.rsplit_once('.').map(|(base, _)| base).unwrap_or(file);
                name.replace('-', " ")
            }
            WallpaperEntry::Object {
                name,
                file_name,
                file_main_name,
                download_url,
                ..
            } => {
                if let Some(n) = name {
                    if !n.is_empty() {
                        return n.replace('-', " ");
                    }
                }
                if let Some(f) = file_name {
                    if !f.is_empty() {
                        return f.replace('-', " ");
                    }
                }
                if let Some(f) = file_main_name {
                    let base = f.rsplit_once('.').map(|(b, _)| b).unwrap_or(f);
                    return base.replace('-', " ");
                }
                if let Some(u) = download_url {
                    let file = u.rsplit('/').next().unwrap_or(u);
                    let base = file.rsplit_once('.').map(|(b, _)| b).unwrap_or(file);
                    return base.replace('-', " ");
                }
                "Wallpaper".into()
            }
        }
    }

    /// Stable identity used to dedupe wallpapers that have already been applied.
    /// Prefers the API's public id / filename fields so the same wallpaper keeps
    /// the same key across searches and repeated calls.
    pub fn id(&self) -> String {
        let raw = match self {
            WallpaperEntry::Url(u) => u.as_str(),
            WallpaperEntry::Object {
                name,
                file_main_name,
                file_name,
                download_url,
                ..
            } => name
                .as_deref()
                .filter(|v| !v.trim().is_empty())
                .or_else(|| file_main_name.as_deref().filter(|v| !v.trim().is_empty()))
                .or_else(|| file_name.as_deref().filter(|v| !v.trim().is_empty()))
                .or_else(|| download_url.as_deref().filter(|v| !v.trim().is_empty()))
                .unwrap_or(""),
        };
        let key = raw.trim().to_lowercase();
        if key.is_empty() {
            self.title().trim().to_lowercase()
        } else {
            key
        }
    }

    /// Category if available (strips leading `#`).
    pub fn category(&self) -> Option<String> {
        match self {
            WallpaperEntry::Url(_) => None,
            WallpaperEntry::Object { category, .. } => {
                category.as_deref().map(|c| c.trim_start_matches('#').to_lowercase())
            }
        }
    }

    /// Resolution badge (e.g. "4K", "2K", or "1080x2400").
    pub fn resolution(&self) -> Option<String> {
        match self {
            WallpaperEntry::Object {
                resolution,
                width,
                height,
                ..
            } => {
                if let Some(r) = resolution {
                    if !r.is_empty() {
                        return Some(r.clone());
                    }
                }
                if let (Some(w), Some(h)) = (width, height) {
                    return Some(format!("{w}x{h}"));
                }
                None
            }
            _ => None,
        }
    }

    /// Mood of the wallpaper (e.g. "calm", "dark", "vibrant").
    pub fn mood(&self) -> Option<&str> {
        match self {
            WallpaperEntry::Object { data: Some(d), .. } => d.mood.as_deref(),
            _ => None,
        }
    }

    /// Scene description if available from curation metadata.
    pub fn description(&self) -> Option<&str> {
        match self {
            WallpaperEntry::Object { data: Some(d), .. } => d.scene_description.as_deref(),
            _ => None,
        }
    }

    /// Tags describing the wallpaper.
    pub fn tags(&self) -> &[String] {
        match self {
            WallpaperEntry::Object { data: Some(d), .. } => &d.tags,
            _ => &[],
        }
    }

    /// Primary colors identified in the wallpaper.
    pub fn primary_colors(&self) -> &[String] {
        match self {
            WallpaperEntry::Object { data: Some(d), .. } => &d.primary_colors,
            _ => &[],
        }
    }

    /// Whether the wallpaper lists the given color among its primary colors.
    pub fn has_primary_color(&self, color: &str) -> bool {
        let needle = color.to_lowercase();
        self.primary_colors().iter().any(|c| c.to_lowercase() == needle)
    }

    /// Check if wallpaper matches a specific color name.
    pub fn matches_color(&self, color: &str) -> bool {
        let needle = color.to_lowercase();
        self.primary_colors().iter().any(|c| c.to_lowercase() == needle)
            || self.tags().iter().any(|t| t.to_lowercase().contains(&needle))
            || self.title().to_lowercase().contains(&needle)
    }

    /// Check if wallpaper contains any blacklisted color.
    pub fn has_blacklisted_color(&self, blacklist: &[String]) -> bool {
        for bl in blacklist {
            let bl_lower = bl.to_lowercase();
            if self.primary_colors().iter().any(|c| c.to_lowercase() == bl_lower) {
                return true;
            }
        }
        false
    }

    /// Whether this entry is tagged as mobile (or unknown).
    pub fn is_mobile(&self) -> bool {
        match self {
            WallpaperEntry::Url(_) => true,
            WallpaperEntry::Object { orientation, .. } => {
                orientation.as_deref().map(|o| o.eq_ignore_ascii_case("mobile")).unwrap_or(true)
            }
        }
    }
}

/// Envelope for responses with a `wallpapers` field.
#[derive(Debug, Deserialize)]
struct WallpapersWrapper {
    #[serde(default)]
    wallpapers: Vec<WallpaperEntry>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_read(TIMEOUT)
        .timeout_write(TIMEOUT)
        .timeout_connect(CONNECT_TIMEOUT)
        .user_agent("wall/0.3 (Android; Termux)")
        .build()
}

/// Query WallWidgy with combined filters.
/// Hardcoded to `type=mobile` for Android Termux.
pub fn query(
    category: Option<&str>,
    color: Option<&str>,
    count: u32,
) -> Result<Vec<WallpaperEntry>, String> {
    let mut url = format!("{BASE}/wallpapers?type={DEVICE}&count={count}");
    if let Some(c) = category {
        let clean = c.trim_start_matches('#');
        url.push_str(&format!("&category={clean}"));
    }
    if let Some(c) = color {
        url.push_str(&format!("&color={c}"));
    }

    let resp = match agent().get(&url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => return Ok(Vec::new()),
        Err(e) => return Err(format!("API request failed: {e}")),
    };

    let wrapper: WallpapersWrapper = resp
        .into_json()
        .map_err(|e| format!("Failed to parse wallpapers response: {e}"))?;

    Ok(wrapper.wallpapers)
}

/// Build a candidate pool for a category/color filter.
///
/// The plain `/wallpapers` filter endpoint returns very few results (e.g. a
/// single wallpaper for `color=black`), which makes repeat avoidance useless.
/// Prefer the search endpoint — it carries color metadata and a much larger
/// pool — then filter locally. Falls back to the plain endpoint if search has
/// nothing usable.
pub fn filtered(
    category: Option<&str>,
    color: Option<&str>,
    count: u32,
) -> Result<Vec<WallpaperEntry>, String> {
    if let Some(c) = color {
        let needle = c.trim().to_lowercase();
        if !needle.is_empty() {
            if let Ok(results) = search(&needle, count) {
                let exact: Vec<WallpaperEntry> = results
                    .iter()
                    .filter(|w| w.has_primary_color(&needle))
                    .cloned()
                    .collect();
                let mut pool = if exact.is_empty() {
                    results
                        .into_iter()
                        .filter(|w| w.matches_color(&needle))
                        .collect()
                } else {
                    exact
                };

                if let Some(cat) = category {
                    let by_category: Vec<WallpaperEntry> = pool
                        .iter()
                        .filter(|w| {
                            w.category()
                                .as_deref()
                                .is_some_and(|wc| wc.eq_ignore_ascii_case(cat))
                        })
                        .cloned()
                        .collect();
                    if !by_category.is_empty() {
                        pool = by_category;
                    }
                }

                if !pool.is_empty() {
                    return Ok(pool);
                }
            }
        }
    }

    query(category, color, count)
}

/// Fetch random mobile wallpapers using the dedicated random API.
pub fn random_many(count: u32) -> Result<Vec<WallpaperEntry>, String> {
    // First try the specialized random endpoint
    let url = format!("{BASE}/random-wallpapers?count={count}");
    if let Ok(resp) = agent().get(&url).call() {
        if let Ok(items) = resp.into_json::<Vec<WallpaperEntry>>() {
            let mobile_only: Vec<_> = items.into_iter().filter(|w| w.is_mobile()).collect();
            if !mobile_only.is_empty() {
                return Ok(mobile_only);
            }
        }
    }

    // Fall back to general endpoint with type=mobile
    query(None, None, count)
}

/// Percent-encode a term for safe use in a query string (spaces, quotes, `&`, non-ASCII, ...).
fn encode_query(term: &str) -> String {
    let mut out = String::with_capacity(term.len());
    for byte in term.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn search_single(term: &str, count: u32) -> Result<Vec<WallpaperEntry>, String> {
    let url = format!("{BASE}/wallpapers/search?q={}&count={count}", encode_query(term));

    let resp = match agent().get(&url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(404, _)) => return Ok(Vec::new()),
        Err(e) => return Err(format!("Search request failed: {e}")),
    };

    let wrapper: WallpapersWrapper = resp
        .into_json()
        .map_err(|e| format!("Failed to parse search response: {e}"))?;

    // Prefer mobile results if available
    let (mut mobile, desktop): (Vec<_>, Vec<_>) =
        wrapper.wallpapers.into_iter().partition(|w| w.is_mobile());
    mobile.extend(desktop);
    Ok(mobile)
}

/// Generate candidate search terms for a query, ordered by likelihood of a hit.
/// WallWidgy indexes hyphenated names and tags, so a multi-word query is first
/// collapsed to its hyphenated form: "one piece" -> "one-piece".
fn search_candidates(query: &str) -> Vec<String> {
    let joined = query.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let trimmed = joined.trim_matches(|c| c == '\'' || c == '"').trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let q = trimmed.to_string();
    if q.contains(' ') {
        vec![q.replace(' ', "-"), q.clone(), q.replace(' ', "")]
    } else {
        vec![q]
    }
}

/// Search wallpapers by keyword or phrase (e.g. "one piece" or "luffy").
/// Tries hyphenated, spaced, and joined variants, then falls back to the
/// individual words (longest first) for queries like "monkey d luffy".
pub fn search(query: &str, count: u32) -> Result<Vec<WallpaperEntry>, String> {
    let candidates = search_candidates(query);
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    let mut last_err: Option<String> = None;
    for candidate in &candidates {
        match search_single(candidate, count) {
            Ok(results) if !results.is_empty() => return Ok(results),
            Ok(_) => {}
            Err(e) => last_err = Some(e),
        }
    }

    // Last resort for multi-word queries: search the individual words,
    // longest first, skipping tiny words. E.g. "monkey d luffy" -> "monkey", "luffy".
    let mut words: Vec<&str> = query.split_whitespace().collect();
    if words.len() > 1 {
        words.retain(|w| w.chars().count() > 2);
        words.sort_by_key(|w| std::cmp::Reverse(w.len()));
        words.dedup();
        for word in words {
            match search_single(word, count) {
                Ok(results) if !results.is_empty() => return Ok(results),
                Ok(_) => {}
                Err(e) => last_err = Some(e),
            }
        }
    }

    // Surface a network/parse failure instead of a misleading "no results".
    match last_err {
        Some(e) => Err(e),
        None => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_multi_word_tries_hyphenated_first() {
        assert_eq!(search_candidates("One Piece"), vec!["one-piece", "one piece", "onepiece"]);
    }

    #[test]
    fn candidates_single_word_unchanged() {
        assert_eq!(search_candidates("  Luffy  "), vec!["luffy"]);
    }

    #[test]
    fn candidates_empty_query() {
        assert!(search_candidates("   ").is_empty());
    }

    #[test]
    fn candidates_strips_surrounding_quotes() {
        assert_eq!(search_candidates("\"one piece\""), vec!["one-piece", "one piece", "onepiece"]);
        assert_eq!(search_candidates("'luffy'"), vec!["luffy"]);
    }

    #[test]
    fn candidates_normalizes_inner_whitespace() {
        assert_eq!(search_candidates("monkey\t d\nluffy"), vec!["monkey-d-luffy", "monkey d luffy", "monkeydluffy"]);
    }

    #[test]
    fn has_primary_color_is_strict() {
        let entry: WallpaperEntry = serde_json::from_str(
            r#"{
                "file_name": "x",
                "data": { "primary_colors": ["black", "gray"], "tags": ["neon", "dark"] }
            }"#,
        )
        .unwrap();
        assert!(entry.has_primary_color("Black"));
        assert!(!entry.has_primary_color("neon"));
    }

    #[test]
    fn id_prefers_public_id_and_is_normalized() {
        let entry: WallpaperEntry = serde_json::from_str(
            r#"{
                "name": "Luffy-Gear-Five",
                "file_name": "some-file.png",
                "download_url": "https://example.com/other.png",
                "category": "amoled"
            }"#,
        )
        .unwrap();
        assert_eq!(entry.id(), "luffy-gear-five");
    }

    #[test]
    fn encode_query_escapes_specials_but_keeps_hyphens() {
        assert_eq!(encode_query("one piece"), "one%20piece");
        assert_eq!(encode_query("one-piece"), "one-piece");
        assert_eq!(encode_query("zoro & sanji"), "zoro%20%26%20sanji");
        assert_eq!(encode_query("\"quoted\""), "%22quoted%22");
        assert_eq!(encode_query("luffy.is#1"), "luffy.is%231");
    }
}

/// Fetch available colors from the API or fallback to official palette.
pub fn available_colors() -> Vec<String> {
    if let Ok(resp) = agent().get(&format!("{BASE}/colors")).call() {
        if let Ok(cols) = resp.into_json::<Vec<String>>() {
            if !cols.is_empty() {
                return cols;
            }
        }
    }
    OFFICIAL_COLORS.iter().map(|s| s.to_string()).collect()
}

/// Download full, uncompressed image bytes without artificial data caps.
pub fn download_image(url: &str) -> Result<Vec<u8>, String> {
    let resp = agent()
        .get(url)
        .call()
        .map_err(|e| format!("Download failed ({url}): {e}"))?;

    let expected_len: usize = resp
        .header("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(6 * 1024 * 1024);

    let mut buf = Vec::with_capacity(expected_len);
    // No artificial size cap — full high-res 4K wallpapers stream completely
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| format!("Failed reading image data: {e}"))?;

    Ok(buf)
}

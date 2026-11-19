//! Applied-wallpaper history — persists the ids of wallpapers that have already
//! been applied so every selection mode can avoid picking the same one twice.
//!
//! Stored as JSON at `~/.cache/wall/history.json`. The list is bounded; once it
//! exceeds `MAX_HISTORY` entries the oldest ones are dropped so very old
//! wallpapers can naturally re-enter the pool.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Maximum number of applied wallpaper ids to remember.
const MAX_HISTORY: usize = 500;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct HistoryFile {
    #[serde(default)]
    applied: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct History {
    path: PathBuf,
    applied: Vec<String>,
}

impl History {
    /// Load the history file. A missing or corrupt file simply starts fresh.
    pub fn load() -> Self {
        let path = crate::config::history_path();
        let applied = fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<HistoryFile>(&s).ok())
            .map(|f| f.applied)
            .unwrap_or_default();
        Self { path, applied }
    }

    /// Path of the backing history file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Number of recorded wallpapers.
    pub fn len(&self) -> usize {
        self.applied.len()
    }

    /// The most recently applied wallpaper id, if any.
    pub fn last(&self) -> Option<&str> {
        self.applied.last().map(|s| s.as_str())
    }

    pub fn contains(&self, id: &str) -> bool {
        self.applied.iter().any(|s| s == id)
    }

    /// Record an applied wallpaper id, moving it to the end of the list.
    pub fn record(&mut self, id: &str) {
        let id = id.trim();
        if id.is_empty() {
            return;
        }
        self.applied.retain(|s| s != id);
        self.applied.push(id.to_string());
        if self.applied.len() > MAX_HISTORY {
            let excess = self.applied.len() - MAX_HISTORY;
            self.applied.drain(..excess);
        }
    }

    /// Persist the current list to disk.
    pub fn save(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = HistoryFile {
            applied: self.applied.clone(),
        };
        let contents = serde_json::to_string_pretty(&data)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        let temp = self.path.with_extension("tmp");
        fs::write(&temp, contents)?;
        fs::rename(temp, &self.path)
    }

    /// Forget every applied wallpaper and remove the history file.
    pub fn clear(&mut self) -> io::Result<()> {
        self.applied.clear();
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_history() -> History {
        History {
            path: PathBuf::from("/nonexistent/history.json"),
            applied: Vec::new(),
        }
    }

    #[test]
    fn record_dedupes_and_moves_to_end() {
        let mut h = empty_history();
        h.record("a");
        h.record("b");
        h.record("a");
        assert_eq!(h.len(), 2);
        assert_eq!(h.last(), Some("a"));
        assert!(h.contains("b"));
    }

    #[test]
    fn record_caps_history_and_drops_oldest() {
        let mut h = empty_history();
        for i in 0..(MAX_HISTORY + 10) {
            h.record(&i.to_string());
        }
        assert_eq!(h.len(), MAX_HISTORY);
        assert!(!h.contains("0"));
        let newest = (MAX_HISTORY + 9).to_string();
        assert_eq!(h.last(), Some(newest.as_str()));
    }
}

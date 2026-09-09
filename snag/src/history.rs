//! What Snag has downloaded, remembered across runs.
//!
//! The queue remembers what has not happened yet; this remembers what has.
//! History is the thing you go looking for a week later when you want that
//! file again, or the link it came from.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::settings::{Mode, Settings};

/// Keeps the file from growing without bound on a busy machine.
const MAX_ENTRIES: usize = 500;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub url: String,
    pub title: String,
    pub mode: Mode,
    pub file: Option<PathBuf>,
    /// Final size in bytes, when we saw one.
    pub bytes: f64,
    pub finished_unix: u64,
}

impl Entry {
    /// The file is only worth offering if it is still where we left it.
    pub fn file_exists(&self) -> bool {
        self.file.as_ref().is_some_and(|f| f.is_file())
    }

    pub fn display_name(&self) -> &str {
        if self.title.is_empty() {
            &self.url
        } else {
            &self.title
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
pub struct History {
    pub entries: Vec<Entry>,
}

impl History {
    pub fn path() -> PathBuf {
        Settings::config_dir().join("history.json")
    }

    /// Read the history, treating an unreadable file as an empty one. Losing
    /// history is a nuisance rather than a disaster, so it is not worth
    /// interrupting the user over.
    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        std::fs::create_dir_all(Settings::config_dir()).map_err(|e| e.to_string())?;
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(Self::path(), raw).map_err(|e| e.to_string())
    }

    /// Record a finished download, newest first.
    pub fn record(&mut self, entry: Entry) {
        // A repeat download of the same link replaces the older record rather
        // than stacking up next to it.
        self.entries.retain(|e| e.url != entry.url);
        self.entries.insert(0, entry);
        self.entries.truncate(MAX_ENTRIES);
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.entries.len() {
            self.entries.remove(index);
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, History, MAX_ENTRIES};
    use crate::settings::Mode;

    fn entry(url: &str) -> Entry {
        Entry {
            url: url.to_string(),
            title: format!("title for {url}"),
            mode: Mode::Auto,
            file: None,
            bytes: 1.0,
            finished_unix: 0,
        }
    }

    #[test]
    fn newest_first() {
        let mut h = History::default();
        h.record(entry("a"));
        h.record(entry("b"));
        assert_eq!(h.entries[0].url, "b");
        assert_eq!(h.entries[1].url, "a");
    }

    #[test]
    fn downloading_the_same_link_again_moves_it_up_rather_than_duplicating() {
        let mut h = History::default();
        h.record(entry("a"));
        h.record(entry("b"));
        h.record(entry("a"));
        assert_eq!(h.entries.len(), 2);
        assert_eq!(h.entries[0].url, "a");
    }

    #[test]
    fn stays_bounded() {
        let mut h = History::default();
        for i in 0..(MAX_ENTRIES + 50) {
            h.record(entry(&i.to_string()));
        }
        assert_eq!(h.entries.len(), MAX_ENTRIES);
        // The oldest are the ones dropped.
        assert_eq!(h.entries[0].url, (MAX_ENTRIES + 49).to_string());
    }
}

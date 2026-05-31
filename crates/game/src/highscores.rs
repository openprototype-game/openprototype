//! Persistent high-score table.
//!
//! The original keeps `HIGH.TXT` next to itself and rewrites it in place. Our
//! disc image is read-only, so the writable copy lives in the OS data directory
//! instead, with the disc's shipped table as the fallback until the player has
//! saved their own. Decoding lives in [`prototype_formats::high`]; this is just
//! the I/O around it.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;
use prototype_disc::{AssetSource, DiscImage};
use prototype_formats::Highscores;

/// The high-score file's name in the data directory.
const FILE_NAME: &str = "high.txt";

pub struct HighscoreStore {
    path: PathBuf,
    disc_default: Highscores,
}

impl HighscoreStore {
    /// Resolve the data-directory path and decode the disc's default table to
    /// fall back on when there is no local copy.
    pub fn open(disc: &DiscImage) -> Result<Self> {
        let dirs = ProjectDirs::from("de", "dasprids", "OpenPrototype")
            .context("resolving the data directory")?;

        Ok(Self {
            path: dirs.data_dir().join(FILE_NAME),
            disc_default: load_disc_default(disc)?,
        })
    }

    /// The current table: the local file if it reads and parses, otherwise the
    /// disc default. Infallible, the fallback is always in memory.
    pub fn load(&self) -> Highscores {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|contents| contents.parse().ok())
            .unwrap_or_else(|| self.disc_default.clone())
    }

    /// Write the table to the local file, creating the data directory if needed.
    pub fn save(&self, scores: &Highscores) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }

        fs::write(&self.path, scores.to_string())
            .with_context(|| format!("writing {}", self.path.display()))
    }
}

/// Decode the disc's shipped `HIGH.TXT`, the seed for a fresh install.
fn load_disc_default(disc: &DiscImage) -> Result<Highscores> {
    let bytes = disc
        .read("HIGH.TXT")
        .context("reading HIGH.TXT from the disc")?;
    let text = std::str::from_utf8(&bytes).context("HIGH.TXT is not text")?;
    text.parse::<Highscores>().context("decoding HIGH.TXT")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic table (no shipped data) by formatting and parsing.
    fn scores(entries: [(&str, u32); 8]) -> Highscores {
        entries
            .iter()
            .map(|(name, score)| format!("{:.<13} {:06}$\n", name, score))
            .collect::<String>()
            .parse()
            .expect("synthetic table parses")
    }

    fn default_scores() -> Highscores {
        scores([
            ("AAA", 8),
            ("BBB", 7),
            ("CCC", 6),
            ("DDD", 5),
            ("EEE", 4),
            ("FFF", 3),
            ("GGG", 2),
            ("HHH", 1),
        ])
    }

    fn saved_scores() -> Highscores {
        scores([
            ("ZZZ", 80),
            ("YYY", 70),
            ("XXX", 60),
            ("WWW", 50),
            ("VVV", 40),
            ("UUU", 30),
            ("TTT", 20),
            ("SSS", 10),
        ])
    }

    fn store_at(path: PathBuf) -> HighscoreStore {
        HighscoreStore {
            path,
            disc_default: default_scores(),
        }
    }

    #[test]
    fn loads_the_default_when_there_is_no_local_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_at(dir.path().join("high.txt"));

        assert_eq!(store.load(), default_scores());
    }

    #[test]
    fn save_then_load_returns_the_saved_table() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_at(dir.path().join("high.txt"));

        store.save(&saved_scores()).unwrap();

        assert_eq!(store.load(), saved_scores());
    }

    #[test]
    fn falls_back_to_the_default_when_the_local_file_is_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("high.txt");
        fs::write(&path, "not a high-score table").unwrap();

        assert_eq!(store_at(path).load(), default_scores());
    }

    #[test]
    fn save_creates_the_data_directory() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_at(dir.path().join("nested").join("high.txt"));

        store.save(&saved_scores()).unwrap();

        assert_eq!(store.load(), saved_scores());
    }
}

//! The savegame slots on disk.
//!
//! The original keeps `PROTOSG1.PSG`..`PROTOSG5.PSG` next to `START.EXE` and
//! treats a slot as occupied when its file exists. The port's slots live in
//! the OS data directory instead (the disc image is read-only), next to the
//! high-score table. Encoding and decoding live in [`crate::savegame`]; this
//! is just the I/O around the five slots.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::ProjectDirs;

use crate::savegame::SaveGame;

/// The slot count; the menus label them GAME 1..5.
pub const SLOTS: usize = 5;

pub struct SaveStore {
    dir: PathBuf,
}

impl SaveStore {
    /// Resolve the data-directory path. No I/O happens until a slot is read
    /// or written.
    pub fn open() -> Result<Self> {
        let dirs = ProjectDirs::from("de", "dasprids", "OpenPrototype")
            .context("resolving the data directory")?;

        Ok(Self {
            dir: dirs.data_dir().to_path_buf(),
        })
    }

    /// The file behind `slot` (0-based; the original numbers them 1..5).
    fn slot_path(&self, slot: usize) -> PathBuf {
        self.dir.join(format!("protosg{}.psg", slot + 1))
    }

    /// Which slots hold a save: the original's occupied flags, here file
    /// existence.
    pub fn occupied(&self) -> [bool; SLOTS] {
        std::array::from_fn(|slot| self.slot_path(slot).exists())
    }

    /// Read and decode the save in `slot`.
    pub fn load(&self, slot: usize) -> Result<SaveGame> {
        let path = self.slot_path(slot);
        let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;

        SaveGame::decode(&bytes).with_context(|| format!("decoding {}", path.display()))
    }

    /// Encode `save` into `slot`, creating the data directory if needed.
    /// Panics on a shooter save, like [`SaveGame::encode`], until that codec
    /// lands.
    pub fn save(&self, slot: usize, save: &SaveGame) -> Result<()> {
        fs::create_dir_all(&self.dir)
            .with_context(|| format!("creating {}", self.dir.display()))?;
        let path = self.slot_path(slot);

        fs::write(&path, save.encode()).with_context(|| format!("writing {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_at(dir: PathBuf) -> SaveStore {
        SaveStore { dir }
    }

    fn race_save() -> SaveGame {
        SaveGame::decode(include_bytes!("../tests/fixtures/l2-race.psg"))
            .expect("the ground-truth fixture decodes")
    }

    #[test]
    fn a_fresh_store_has_no_occupied_slots() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_at(dir.path().join("data"));

        assert_eq!(store.occupied(), [false; SLOTS]);
    }

    #[test]
    fn saving_occupies_only_that_slot_and_loads_back() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_at(dir.path().join("data"));
        let save = race_save();

        store.save(2, &save).unwrap();

        assert_eq!(store.occupied(), [false, false, true, false, false]);
        assert_eq!(store.load(2).unwrap(), save);
    }

    #[test]
    fn loading_an_empty_slot_fails() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_at(dir.path().to_path_buf());

        assert!(store.load(0).is_err());
    }

    #[test]
    fn the_slot_files_carry_the_original_names() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_at(dir.path().to_path_buf());

        store.save(0, &race_save()).unwrap();

        assert!(dir.path().join("protosg1.psg").exists());
    }
}

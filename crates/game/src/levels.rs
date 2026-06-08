//! Per-level data registry.
//!
//! Each level differs in which WAD it loads and, as more is reverse-engineered,
//! its parallax background, scenery layers, and palette. [`Level::data`] is the
//! one place those per-level facts live, so loaders key off it instead of
//! hardcoding `LEVEL_1.WAD`.

use crate::background::Sp;

/// One of the seven levels.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    L1,
    L2,
    L3,
    L4,
    L5,
    L6,
    L7,
}

/// The per-level facts the loaders need. Cheap to copy; it grows new fields as
/// more per-level data is reverse-engineered.
#[derive(Clone, Copy)]
pub struct LevelData {
    /// The level's WAD/executable on the disc, e.g. `"LEVEL_2.WAD"`.
    pub wad: &'static str,
    /// The level's SP parallax background (which carries its own strip layout).
    pub background: Sp,
}

impl Level {
    /// This level's data. Exhaustive, so a new [`Level`] variant must supply its
    /// own entry.
    pub fn data(self) -> LevelData {
        match self {
            Level::L1 => LevelData {
                wad: "LEVEL_1.WAD",
                background: Sp::Canyon,
            },
            Level::L2 => LevelData {
                wad: "LEVEL_2.WAD",
                background: Sp::Raceb2,
            },
            Level::L3 => LevelData {
                wad: "LEVEL_3.WAD",
                background: Sp::Wald,
            },
            Level::L4 => LevelData {
                wad: "LEVEL_4.WAD",
                background: Sp::Raceb2,
            },
            Level::L5 => LevelData {
                wad: "LEVEL_5.WAD",
                background: Sp::Alienbg,
            },
            Level::L6 => LevelData {
                wad: "LEVEL_6.WAD",
                background: Sp::Raceb2,
            },
            Level::L7 => LevelData {
                wad: "LEVEL_7.WAD",
                background: Sp::Lavah,
            },
        }
    }
}

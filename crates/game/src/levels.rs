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

/// The five sprite-catalog BIN files. Like the SP backgrounds, the four shooter
/// levels each have their own; the race levels (2/4/6) share Race1.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bin {
    Out,
    Race1,
    Wald,
    Techno,
    Lava,
}

impl Bin {
    /// The `.BIN` filename stem on the disc.
    pub fn stem(self) -> &'static str {
        match self {
            Bin::Out => "OUT",
            Bin::Race1 => "RACE1",
            Bin::Wald => "WALD",
            Bin::Techno => "TECHNO",
            Bin::Lava => "LAVA",
        }
    }
}

/// The per-level facts the loaders need. Cheap to copy; it grows new fields as
/// more per-level data is reverse-engineered.
#[derive(Clone, Copy)]
pub struct LevelData {
    /// The level's WAD/executable on the disc, e.g. `"LEVEL_2.WAD"`.
    pub wad: &'static str,
    /// The level's SP parallax background (which carries its own strip layout).
    pub background: Sp,
    /// The level's sprite-catalog BIN file (per-theme; 2/4/6 share Race1).
    pub catalog: Bin,
    /// File offset of the catalog descriptor table in the level's WAD.
    pub catalog_offset: usize,
}

impl Level {
    /// The level numbered `1..=7`, if in range.
    pub fn from_number(number: u8) -> Option<Level> {
        match number {
            1 => Some(Level::L1),
            2 => Some(Level::L2),
            3 => Some(Level::L3),
            4 => Some(Level::L4),
            5 => Some(Level::L5),
            6 => Some(Level::L6),
            7 => Some(Level::L7),
            _ => None,
        }
    }

    /// This level's data. Exhaustive, so a new [`Level`] variant must supply its
    /// own entry.
    pub fn data(self) -> LevelData {
        match self {
            Level::L1 => LevelData {
                wad: "LEVEL_1.WAD",
                background: Sp::Canyon,
                catalog: Bin::Out,
                catalog_offset: 0xf9f0,
            },
            Level::L2 => LevelData {
                wad: "LEVEL_2.WAD",
                background: Sp::Raceb2,
                catalog: Bin::Race1,
                catalog_offset: 0xbf5a,
            },
            Level::L3 => LevelData {
                wad: "LEVEL_3.WAD",
                background: Sp::Wald,
                catalog: Bin::Wald,
                catalog_offset: 0x134c0,
            },
            Level::L4 => LevelData {
                wad: "LEVEL_4.WAD",
                background: Sp::Raceb2,
                catalog: Bin::Race1,
                catalog_offset: 0xbfd6,
            },
            Level::L5 => LevelData {
                wad: "LEVEL_5.WAD",
                background: Sp::Alienbg,
                catalog: Bin::Techno,
                catalog_offset: 0x10e10,
            },
            Level::L6 => LevelData {
                wad: "LEVEL_6.WAD",
                background: Sp::Raceb2,
                catalog: Bin::Race1,
                catalog_offset: 0xc4d6,
            },
            Level::L7 => LevelData {
                wad: "LEVEL_7.WAD",
                background: Sp::Lavah,
                catalog: Bin::Lava,
                catalog_offset: 0x13240,
            },
        }
    }
}

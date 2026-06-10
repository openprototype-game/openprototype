//! Per-level data registry.
//!
//! Each level differs in which WAD it loads and, as more is reverse-engineered,
//! its parallax background, scenery layers, and palette. [`Level::data`] is the
//! one place those per-level facts live, so loaders key off it instead of
//! hardcoding `LEVEL_1.WAD`.

use crate::background::Sp;
use openprototype_core::PerWeapon;

/// Which catalog cells make up a weapon's overlay sprite: the run of `count`
/// consecutive cells starting at `first`.
#[derive(Clone, Copy)]
pub struct Overlay {
    pub first: usize,
    pub count: usize,
}

/// One scenery layer's source: where its tilemap stream starts in the WAD
/// (`cs`-relative), the screen row it draws from, and its scroll speed in
/// 1/16-pixel units per tick.
#[derive(Clone, Copy)]
pub struct SceneryLayerData {
    pub cs_offset: usize,
    pub top: i32,
    pub speed: u32,
}

/// One plane of a level's star field: 30 single-pixel stars sweeping left at
/// `speed` (1/16-pixel per tick), plotted in palette index `color`.
#[derive(Clone, Copy)]
pub struct StarPlaneData {
    pub speed: u32,
    pub color: u8,
    /// The plane plots only onto still-black pixels, so it never overwrites the
    /// background art (the original's depth cue for its dimmest plane).
    pub only_on_black: bool,
    /// Whether the level init scatters this plane. The original's initializer
    /// skips one of L2's four tables, leaving its 30 stars stacked at the
    /// origin as a single drifting pixel; that quirk is kept faithfully.
    pub seeded: bool,
}

/// A level's scenery: the segment-to-file base for its WAD (`file = cs_offset +
/// cs_base`), the cell-base offset, and its layers, back to front. The asset
/// loader decodes this into renderable layers.
#[derive(Clone, Copy)]
pub struct SceneryData {
    pub cs_base: usize,
    /// Added to each stream byte to get its catalog cell index, so the
    /// per-level render routine's cell base maps onto our `decode_banked` sprite
    /// indices (L1 `-1`, L2 `968`; stubbed levels `0`).
    pub cell_base: i32,
    pub layers: &'static [SceneryLayerData],
}

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
    /// Each weapon's overlay cells in [`LevelData::catalog`]. Found by
    /// content-matching L1's overlay sprites in each level's catalog. The
    /// chaingun has no overlay, so it has no entry.
    pub overlays: PerWeapon<Overlay>,
    /// The level's parallax scenery. L1 and L2 are reverse-engineered; 3-7 use
    /// the same engine but are still stubbed (`cs_base: 0`, no layers) until
    /// their layer composition is mapped.
    pub scenery: SceneryData,
    /// The level's star-field planes, drawn between the parallax background
    /// and the scenery, back to front. Empty for levels without one (or not
    /// yet reverse-engineered).
    pub stars: &'static [StarPlaneData],
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
                overlays: PerWeapon {
                    multishot: Overlay {
                        first: 0xEB,
                        count: 1,
                    },
                    burning: Overlay {
                        first: 0xED,
                        count: 1,
                    },
                    plasma: Overlay {
                        first: 0xEE,
                        count: 2,
                    },
                    missile: Overlay {
                        first: 0xF0,
                        count: 2,
                    },
                },
                scenery: SceneryData {
                    cs_base: 0x29F0,
                    cell_base: -1,
                    // Tilemaps cs:0x3137/0x30f2/0x3178 (back/mid/front); top is
                    // the Mode X dest offset over 80, speed the parallax rate.
                    layers: &[
                        SceneryLayerData {
                            cs_offset: 0x3137,
                            top: 38,
                            speed: 6,
                        },
                        SceneryLayerData {
                            cs_offset: 0x30f2,
                            top: 14,
                            speed: 10,
                        },
                        SceneryLayerData {
                            cs_offset: 0x3178,
                            top: 4,
                            speed: 16,
                        },
                    ],
                },
                stars: &[],
            },
            Level::L2 => LevelData {
                wad: "LEVEL_2.WAD",
                background: Sp::Raceb2,
                catalog: Bin::Race1,
                catalog_offset: 0xbf5a,
                overlays: PerWeapon {
                    multishot: Overlay {
                        first: 0x358,
                        count: 1,
                    },
                    burning: Overlay {
                        first: 0x357,
                        count: 1,
                    },
                    plasma: Overlay {
                        first: 0x359,
                        count: 2,
                    },
                    missile: Overlay {
                        first: 0x35b,
                        count: 2,
                    },
                },
                scenery: SceneryData {
                    cs_base: 0x09B0,
                    cell_base: 968,
                    // L2 keeps only the first of the engine's three layer slots
                    // live: the frame renderer sets up the row-14 and row-4
                    // streams too, but never calls the walker for them (dead
                    // code), so the race structures at row 38 are the level's
                    // whole scenery.
                    layers: &[SceneryLayerData {
                        cs_offset: 0x34e9,
                        top: 38,
                        speed: 48,
                    }],
                },
                // Four blue star planes over the nebula; brightness tracks
                // speed (the nebula itself scrolls at 32, so one plane drifts
                // behind it and two ahead). The second table is the one the
                // original never seeds.
                stars: &[
                    StarPlaneData {
                        speed: 0x1c,
                        color: 0x8d,
                        only_on_black: true,
                        seeded: true,
                    },
                    StarPlaneData {
                        speed: 0x24,
                        color: 0x8b,
                        only_on_black: false,
                        seeded: false,
                    },
                    StarPlaneData {
                        speed: 0x28,
                        color: 0x89,
                        only_on_black: false,
                        seeded: true,
                    },
                    StarPlaneData {
                        speed: 0x2c,
                        color: 0x87,
                        only_on_black: false,
                        seeded: true,
                    },
                ],
            },
            Level::L3 => LevelData {
                wad: "LEVEL_3.WAD",
                background: Sp::Wald,
                catalog: Bin::Wald,
                catalog_offset: 0x134c0,
                overlays: PerWeapon {
                    multishot: Overlay {
                        first: 0xa1,
                        count: 1,
                    },
                    burning: Overlay {
                        first: 0xa0,
                        count: 1,
                    },
                    plasma: Overlay {
                        first: 0xa2,
                        count: 2,
                    },
                    missile: Overlay {
                        first: 0xa4,
                        count: 2,
                    },
                },
                // TODO: reverse-engineer this level's scenery (cs_base + layers).
                scenery: SceneryData {
                    cs_base: 0,
                    cell_base: 0,
                    layers: &[],
                },
                stars: &[],
            },
            Level::L4 => LevelData {
                wad: "LEVEL_4.WAD",
                background: Sp::Raceb2,
                catalog: Bin::Race1,
                catalog_offset: 0xbfd6,
                overlays: PerWeapon {
                    multishot: Overlay {
                        first: 0x362,
                        count: 1,
                    },
                    burning: Overlay {
                        first: 0x361,
                        count: 1,
                    },
                    plasma: Overlay {
                        first: 0x363,
                        count: 2,
                    },
                    missile: Overlay {
                        first: 0x365,
                        count: 2,
                    },
                },
                // TODO: reverse-engineer this level's scenery (cs_base + layers).
                scenery: SceneryData {
                    cs_base: 0,
                    cell_base: 0,
                    layers: &[],
                },
                stars: &[],
            },
            Level::L5 => LevelData {
                wad: "LEVEL_5.WAD",
                background: Sp::Alienbg,
                catalog: Bin::Techno,
                catalog_offset: 0x10e10,
                overlays: PerWeapon {
                    multishot: Overlay {
                        first: 0xa1,
                        count: 1,
                    },
                    burning: Overlay {
                        first: 0xa0,
                        count: 1,
                    },
                    plasma: Overlay {
                        first: 0xa2,
                        count: 2,
                    },
                    missile: Overlay {
                        first: 0xa4,
                        count: 2,
                    },
                },
                // TODO: reverse-engineer this level's scenery (cs_base + layers).
                scenery: SceneryData {
                    cs_base: 0,
                    cell_base: 0,
                    layers: &[],
                },
                stars: &[],
            },
            Level::L6 => LevelData {
                wad: "LEVEL_6.WAD",
                background: Sp::Raceb2,
                catalog: Bin::Race1,
                catalog_offset: 0xc4d6,
                overlays: PerWeapon {
                    multishot: Overlay {
                        first: 0x3e2,
                        count: 1,
                    },
                    burning: Overlay {
                        first: 0x3e1,
                        count: 1,
                    },
                    plasma: Overlay {
                        first: 0x3e3,
                        count: 2,
                    },
                    missile: Overlay {
                        first: 0x3e5,
                        count: 2,
                    },
                },
                // TODO: reverse-engineer this level's scenery (cs_base + layers).
                scenery: SceneryData {
                    cs_base: 0,
                    cell_base: 0,
                    layers: &[],
                },
                stars: &[],
            },
            Level::L7 => LevelData {
                wad: "LEVEL_7.WAD",
                background: Sp::Lavah,
                catalog: Bin::Lava,
                catalog_offset: 0x13240,
                overlays: PerWeapon {
                    multishot: Overlay {
                        first: 0xa1,
                        count: 1,
                    },
                    burning: Overlay {
                        first: 0xa0,
                        count: 1,
                    },
                    plasma: Overlay {
                        first: 0xa2,
                        count: 2,
                    },
                    missile: Overlay {
                        first: 0xa4,
                        count: 2,
                    },
                },
                // TODO: reverse-engineer this level's scenery (cs_base + layers).
                scenery: SceneryData {
                    cs_base: 0,
                    cell_base: 0,
                    layers: &[],
                },
                stars: &[],
            },
        }
    }
}

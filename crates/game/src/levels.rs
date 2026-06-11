//! Per-level data registry.
//!
//! Each level differs in which WAD it loads and, as more is reverse-engineered,
//! its parallax background, scenery layers, and palette. [`Level::data`] is the
//! one place those per-level facts live, so loaders key off it instead of
//! hardcoding `LEVEL_1.WAD`.

use crate::background::Sp;
use crate::level::spawn::SpawnSource;
use crate::level::{level_1, level_3, level_5, level_7};
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

/// A level's ship table: where its frame catalog sits in the WAD, and which
/// frame is level flight.
///
/// Every level shares `PTURN1.BN1` and the same 27-frame barrel-roll cycle,
/// but the levels disagree on the camera angle of level flight: most idle on
/// frame 0 (the top-down view) and flicker their exhaust via an extra frame at
/// 27; L3 and L5 idle on frame 21 (the side view) and carry a 29th catalog
/// frame (28, idle + 7) as its exhaust variant. The roll handler returns to
/// `idle_frame` by the shortest way around the cycle in every level; the ship
/// still spawns on frame 0, so the side-view levels visibly roll into their
/// pose during the fly-in.
#[derive(Clone, Copy)]
pub struct ShipData {
    /// File offset of the frame catalog in the level's WAD (L1 `cs:0x43ea` +
    /// 2, the `decode_ship` convention): two-cell frames of `PTURN1.BN1`
    /// plane pointers.
    pub catalog: usize,
    /// The level-flight frame the roll returns to (`cmp` target in the
    /// no-key branch of the roll handler).
    pub idle_frame: usize,
    /// The idle exhaust-flicker alternate frame (the draw site's `add di`
    /// over 18).
    pub flicker_frame: usize,
    /// The vertical clamps (`cmp` guards before the 2-pixel steps): L1 stops
    /// higher (-2..110), L5 at 113, the rest fly -12..120. The horizontal
    /// clamps are -12..230 in every level.
    pub y_min: i32,
    pub y_max: i32,
    /// Shield ticks granted at spawn: 300 (~5s) in L1 and L5, 180 (~3s)
    /// elsewhere.
    pub spawn_shield_ticks: i32,
}

/// Where a level's WAD keeps the player-fire data: the shot sprites' directory
/// records (`{ncells, width, height, cell}`, like the shield's), the chaingun
/// muzzle-flash directory, and the per-roll-frame barrel offsets. All file
/// offsets, located per WAD by spawner-code byte scans
/// (`re/find_shot_dirs.py`, `re/find_weapon_tables.py`); the spawn offsets,
/// velocities, damages and fire rates are identical in all seven WADs and
/// live in [`crate::shots`].
#[derive(Clone, Copy)]
pub struct FireData {
    /// The chaingun shot's directory record.
    pub chaingun: usize,
    /// The multishot projectile's record per charge level 1..=4.
    pub multishot: [usize; 4],
    /// The burning beam's record per charge level 1..=4.
    pub burning: [usize; 4],
    /// The plasma orbs' bolt record.
    pub plasma_bolt: usize,
    /// The missile's record.
    pub missile: usize,
    /// The chaingun muzzle flash: 6 records (3 cells, 2 frames each).
    pub muzzle_flash: usize,
    /// The four plasma orbs' sprite bases, each 4 animation-frame records.
    pub plasma_orbs: [usize; 4],
    /// The orbs' bob wave: a repeating table of signed words the orbs sample
    /// at staggered phases.
    pub bob_table: usize,
    /// 27 `(i16, i16)` pairs: the two barrel y offsets per roll frame, used
    /// by the chaingun/multishot shot spawns and the muzzle flash.
    pub barrel_table: usize,
}

/// A level's sound effects: where its `.SMP` filename table sits in the WAD,
/// and how much of each sample its triggers play.
///
/// Each WAD bakes in a table of 16-byte, NUL-padded sample filenames (17 in
/// L5), loaded whole at level start. The trigger routines play each sample to
/// an authored length constant, around 200 bytes short of the file, with real
/// per-level quirks (L1's multisho stops 2 bytes earlier than the others';
/// extraabg plays 1542 of its 1980 bytes everywhere but L1). Slot meanings are
/// positional and identical across levels, except slot 8, the per-level enemy
/// sound (gegrocke, lgegshot, kanone or scheren), and L5's extra slot 16.
#[derive(Clone, Copy)]
pub struct SfxData {
    /// File offset of the filename table in the level's WAD.
    pub name_table: usize,
    /// Per slot, the authored playback length in bytes (its length doubles as
    /// the slot count).
    pub sample_lengths: &'static [usize],
}

/// A level's scenery: the segment-to-file base for its WAD (`file = cs_offset +
/// cs_base`), the cell-base offset, and its layers, back to front. The asset
/// loader decodes this into renderable layers.
#[derive(Clone, Copy)]
pub struct SceneryData {
    pub cs_base: usize,
    /// Added to each stream byte to get its catalog cell index, so the
    /// per-level render routine's cell base maps onto our `decode_banked` sprite
    /// indices (L1 `-1`, the shooter levels `273`, the race levels
    /// `968`/`978`/`1106`).
    pub cell_base: i32,
    pub layers: &'static [SceneryLayerData],
    /// How many trailing entries of `layers` draw in front of the playfield
    /// sprites (the frame functions call those walkers after the ship/entity
    /// pass): L1's row-4 girders and L3's fast canopy; every other level
    /// draws all scenery behind the ship.
    pub front_layers: usize,
}

/// The race levels' shared star field: four blue planes over the nebula,
/// brightness tracking speed (the nebula itself scrolls at 32, so one plane
/// drifts behind it and two ahead). The engine code and tables are identical
/// in L2/4/6; the second table is the one the original never seeds.
const RACE_STARS: &[StarPlaneData] = &[
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
];

/// L1's per-slot trigger lengths. Slot 8 is gegrocke, and the multisho and
/// extraabg constants differ from every other level's (see [`SfxData`]).
const L1_SFX_LENGTHS: &[usize] = &[
    0x1920, 0x17e, 0x2bfc, 0x1a92, 0x3e74, 0x1840, 0x22ae, 0xc50, 0x2d62, 0x11e4, 0x1e68, 0x1912,
    0x6f4, 0x606, 0x1482, 0x604,
];

/// The race levels' (2/4/6) per-slot trigger lengths. Slot 8 is gegrocke.
const RACE_SFX_LENGTHS: &[usize] = &[
    0x1920, 0x17e, 0x2bfc, 0x1a92, 0x3e74, 0x1840, 0x22ae, 0xc50, 0x2d62, 0x11e4, 0x1e68, 0x1912,
    0x6f4, 0x62c, 0x1482, 0x606,
];

/// L3's per-slot trigger lengths. Slot 8 is lgegshot.
const L3_SFX_LENGTHS: &[usize] = &[
    0x1920, 0x17e, 0x2bfc, 0x1a92, 0x3e74, 0x1840, 0x22ae, 0xc50, 0xa78, 0x11e4, 0x1e68, 0x1912,
    0x6f4, 0x62c, 0x1482, 0x606,
];

/// L5's per-slot trigger lengths. Slot 8 is kanone, and a 17th slot carries
/// lgegshot.
const L5_SFX_LENGTHS: &[usize] = &[
    0x1920, 0x17e, 0x2bfc, 0x1a92, 0x3e74, 0x1840, 0x22ae, 0xc50, 0x1e96, 0x11e4, 0x1e68, 0x1912,
    0x6f4, 0x62c, 0x1482, 0x606, 0xa78,
];

/// L7's per-slot trigger lengths. Slot 8 is scheren.
const L7_SFX_LENGTHS: &[usize] = &[
    0x1920, 0x17e, 0x2bfc, 0x1a92, 0x3e74, 0x1840, 0x22ae, 0xc50, 0x207c, 0x11e4, 0x1e68, 0x1912,
    0x6f4, 0x62c, 0x1482, 0x606,
];

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
    /// File offset of the overlay position table in the level's WAD (L1
    /// `cs:0x9128`): per weapon, the overlay's `(x, y)` screen position for
    /// each settle-animation frame. The table bytes are identical in all seven
    /// WADs, located per level by byte-matching L1's.
    pub overlay_positions: usize,
    /// The level's ship catalog and frame selection (see [`ShipData`]).
    pub ship: ShipData,
    /// File offset of the shield sprite directory in the level's WAD (L1
    /// `cs:0x438a`): 8-byte records `{ncells, width, height, cell}` whose
    /// `cell` indexes [`LevelData::catalog`]. The shield animation cycles the
    /// first [`SHIELD_FRAMES`](crate::ship::SHIELD_FRAMES) records.
    pub shield_directory: usize,
    /// The level's player-fire data (see [`FireData`]).
    pub fire: FireData,
    /// The level's sound-effect data (see [`SfxData`]).
    pub sfx: SfxData,
    /// The level's CD-DA music track, baked into the WAD's data image (byte
    /// `ds:0x632`, the `mov al` before the driver's play call): L1 plays 3,
    /// L3 5, L5 4, L7 6, and the race levels share 7. The engine loops it by
    /// timer: the track's TOC length in 60 Hz ticks counts down in the timer
    /// ISR and an underflow restarts the track.
    pub music_track: u8,
    /// The level's parallax scenery layers, back to front, all reverse-
    /// engineered from each level's WAD.
    pub scenery: SceneryData,
    /// The level's star-field planes, drawn between the parallax background
    /// and the scenery, back to front. Empty for levels without one (or not
    /// yet reverse-engineered).
    pub stars: &'static [StarPlaneData],
    /// The vertical camera's upper stop, from the WAD's clamp code (`cmp` before
    /// the decrement; the lower stop is 32 in every level). The camera also
    /// starts here, per the pan variable's data-image value. Only L3 is panned
    /// down from row 0.
    pub camera_min: i32,
    /// The level's enemy/pickup spawn placement: generated at load by a layout
    /// script (1/3/5/7) or a static table baked into the WAD (2/4/6). See
    /// `reference/formats/level-layout.md`.
    pub spawns: SpawnSource,
    /// The level's spawn-position table in its WAD (the rows a record's
    /// `spawn_row` indexes: `{x, y, movement mode, movement arg}`). `None`
    /// until that level's table is located in its WAD.
    pub spawn_positions: Option<SpawnPositionsData>,
}

/// Where a WAD keeps its spawn-position table: the file offset, the row
/// count (the level's highest `spawn_row` + 1), and which AI set drives its
/// mode-0 rows.
#[derive(Clone, Copy)]
pub struct SpawnPositionsData {
    pub table: usize,
    pub rows: usize,
    /// The level's transcribed AI-function set; `None` leaves entities at
    /// their spawn positions until that level's functions are transcribed.
    pub ai: Option<SpawnAi>,
}

/// A level's transcribed enemy AI-function set (the per-level pointer-table
/// functions mode-0 spawn rows select by `arg`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SpawnAi {
    /// LEVEL_1's 24 functions (`re/l1-ai-functions.md`).
    L1,
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
                overlay_positions: 0xbb18,
                ship: ShipData {
                    catalog: 0x6ddc,
                    y_min: -2,
                    y_max: 110,
                    spawn_shield_ticks: 300,
                    idle_frame: 0,
                    flicker_frame: 27,
                },
                shield_directory: 0x6d7a,
                fire: FireData {
                    chaingun: 0x5be8,
                    multishot: [0x5bf0, 0x5bf8, 0x5c00, 0x5c08],
                    burning: [0x5c10, 0x5c18, 0x5c20, 0x5c28],
                    plasma_bolt: 0x5cb0,
                    missile: 0x5cb8,
                    muzzle_flash: 0x6266,
                    plasma_orbs: [0x5c30, 0x5c50, 0x5c70, 0x5c90],
                    bob_table: 0x55e8,
                    barrel_table: 0xb8dc,
                },
                sfx: SfxData {
                    name_table: 0x5229,
                    sample_lengths: L1_SFX_LENGTHS,
                },
                music_track: 3,
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
                    front_layers: 1,
                },
                stars: &[],
                camera_min: 0,
                spawns: SpawnSource::Generated {
                    script: level_1::script,
                    post_pass: None,
                },
                spawn_positions: Some(SpawnPositionsData {
                    table: 0x5678,
                    rows: 76,
                    ai: Some(SpawnAi::L1),
                }),
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
                overlay_positions: 0x9683,
                ship: ShipData {
                    catalog: 0x4954,
                    y_min: -12,
                    y_max: 120,
                    spawn_shield_ticks: 180,
                    idle_frame: 0,
                    flicker_frame: 27,
                },
                shield_directory: 0x438a,
                fire: FireData {
                    chaingun: 0x3fb8,
                    multishot: [0x3fc0, 0x3fca, 0x3fd4, 0x3fde],
                    burning: [0x3fe8, 0x3ff0, 0x3ff8, 0x4000],
                    plasma_bolt: 0x3f90,
                    missile: 0x4088,
                    muzzle_flash: 0x4188,
                    plasma_orbs: [0x4008, 0x4028, 0x4048, 0x4068],
                    bob_table: 0x3782,
                    barrel_table: 0x9447,
                },
                sfx: SfxData {
                    name_table: 0x33c3,
                    sample_lengths: RACE_SFX_LENGTHS,
                },
                music_track: 7,
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
                    front_layers: 0,
                },
                stars: RACE_STARS,
                camera_min: 0,
                spawns: SpawnSource::StaticTable { table: 0x1690 },
                spawn_positions: None,
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
                overlay_positions: 0xf504,
                ship: ShipData {
                    catalog: 0xa85a,
                    y_min: -2,
                    y_max: 120,
                    spawn_shield_ticks: 180,
                    idle_frame: 21,
                    flicker_frame: 28,
                },
                shield_directory: 0x995e,
                fire: FireData {
                    chaingun: 0x958c,
                    multishot: [0x9594, 0x959e, 0x95a8, 0x95b2],
                    burning: [0x95bc, 0x95c4, 0x95cc, 0x95d4],
                    plasma_bolt: 0x9564,
                    missile: 0x965c,
                    muzzle_flash: 0x975c,
                    plasma_orbs: [0x95dc, 0x95fc, 0x961c, 0x963c],
                    bob_table: 0x85af,
                    barrel_table: 0xf2c4,
                },
                sfx: SfxData {
                    name_table: 0x81f0,
                    sample_lengths: L3_SFX_LENGTHS,
                },
                music_track: 5,
                scenery: SceneryData {
                    cs_base: 0x4710,
                    cell_base: 273,
                    layers: &[
                        SceneryLayerData {
                            cs_offset: 0x4706,
                            top: 1,
                            speed: 10,
                        },
                        SceneryLayerData {
                            cs_offset: 0x4726,
                            top: 3,
                            speed: 16,
                        },
                        SceneryLayerData {
                            cs_offset: 0x495d,
                            top: 1,
                            speed: 32,
                        },
                    ],
                    front_layers: 1,
                },
                stars: &[],
                camera_min: 4,
                spawns: SpawnSource::Generated {
                    script: level_3::script,
                    post_pass: Some(level_3::post_pass),
                },
                spawn_positions: Some(SpawnPositionsData {
                    table: 0x85df,
                    rows: 113,
                    ai: None,
                }),
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
                overlay_positions: 0x96fb,
                ship: ShipData {
                    catalog: 0x49cc,
                    y_min: -12,
                    y_max: 120,
                    spawn_shield_ticks: 180,
                    idle_frame: 0,
                    flicker_frame: 27,
                },
                shield_directory: 0x4402,
                fire: FireData {
                    chaingun: 0x4030,
                    multishot: [0x4038, 0x4042, 0x404c, 0x4056],
                    burning: [0x4060, 0x4068, 0x4070, 0x4078],
                    plasma_bolt: 0x4008,
                    missile: 0x4100,
                    muzzle_flash: 0x4200,
                    plasma_orbs: [0x4080, 0x40a0, 0x40c0, 0x40e0],
                    bob_table: 0x37fa,
                    barrel_table: 0x94bf,
                },
                sfx: SfxData {
                    name_table: 0x343b,
                    sample_lengths: RACE_SFX_LENGTHS,
                },
                music_track: 7,
                // The race levels' tilemap streams are byte-identical; the look
                // differs through cell_base, which points the shared codes at a
                // different window of RACE1.BIN per level.
                scenery: SceneryData {
                    cs_base: 0x09B0,
                    cell_base: 978,
                    layers: &[SceneryLayerData {
                        cs_offset: 0x3561,
                        top: 38,
                        speed: 48,
                    }],
                    front_layers: 0,
                },
                stars: RACE_STARS,
                camera_min: 0,
                spawns: SpawnSource::StaticTable { table: 0x1690 },
                spawn_positions: None,
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
                overlay_positions: 0xd1e0,
                ship: ShipData {
                    catalog: 0x84d0,
                    y_min: -12,
                    y_max: 113,
                    spawn_shield_ticks: 300,
                    idle_frame: 21,
                    flicker_frame: 28,
                },
                shield_directory: 0x775a,
                fire: FireData {
                    chaingun: 0x7388,
                    multishot: [0x7390, 0x739a, 0x73a4, 0x73ae],
                    burning: [0x73b8, 0x73c0, 0x73c8, 0x73d0],
                    plasma_bolt: 0x7360,
                    missile: 0x7458,
                    muzzle_flash: 0x7558,
                    plasma_orbs: [0x73d8, 0x73f8, 0x7418, 0x7438],
                    bob_table: 0x6b86,
                    barrel_table: 0xcfa0,
                },
                sfx: SfxData {
                    name_table: 0x67ad,
                    sample_lengths: L5_SFX_LENGTHS,
                },
                music_track: 4,
                scenery: SceneryData {
                    cs_base: 0x3f90,
                    cell_base: 273,
                    layers: &[
                        SceneryLayerData {
                            cs_offset: 0x3114,
                            top: 50,
                            speed: 8,
                        },
                        SceneryLayerData {
                            cs_offset: 0x315c,
                            top: 78,
                            speed: 16,
                        },
                    ],
                    front_layers: 0,
                },
                stars: &[],
                camera_min: 0,
                spawns: SpawnSource::Generated {
                    script: level_5::script,
                    post_pass: None,
                },
                spawn_positions: Some(SpawnPositionsData {
                    table: 0x6bb6,
                    rows: 99,
                    ai: None,
                }),
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
                overlay_positions: 0x9bfb,
                ship: ShipData {
                    catalog: 0x4ecc,
                    y_min: -12,
                    y_max: 120,
                    spawn_shield_ticks: 180,
                    idle_frame: 0,
                    flicker_frame: 27,
                },
                shield_directory: 0x4902,
                fire: FireData {
                    chaingun: 0x4530,
                    multishot: [0x4538, 0x4542, 0x454c, 0x4556],
                    burning: [0x4560, 0x4568, 0x4570, 0x4578],
                    plasma_bolt: 0x4508,
                    missile: 0x4600,
                    muzzle_flash: 0x4700,
                    plasma_orbs: [0x4580, 0x45a0, 0x45c0, 0x45e0],
                    bob_table: 0x3cfa,
                    barrel_table: 0x99bf,
                },
                sfx: SfxData {
                    name_table: 0x393b,
                    sample_lengths: RACE_SFX_LENGTHS,
                },
                music_track: 7,
                scenery: SceneryData {
                    cs_base: 0x09B0,
                    cell_base: 1106,
                    layers: &[SceneryLayerData {
                        cs_offset: 0x3a61,
                        top: 38,
                        speed: 48,
                    }],
                    front_layers: 0,
                },
                stars: RACE_STARS,
                camera_min: 0,
                spawns: SpawnSource::StaticTable { table: 0x1690 },
                spawn_positions: None,
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
                overlay_positions: 0xf75d,
                ship: ShipData {
                    catalog: 0xaa94,
                    y_min: -12,
                    y_max: 120,
                    spawn_shield_ticks: 180,
                    idle_frame: 0,
                    flicker_frame: 27,
                },
                shield_directory: 0x94d7,
                fire: FireData {
                    chaingun: 0x9105,
                    multishot: [0x910d, 0x9117, 0x9121, 0x912b],
                    burning: [0x9135, 0x913d, 0x9145, 0x914d],
                    plasma_bolt: 0x90dd,
                    missile: 0x91d5,
                    muzzle_flash: 0x92d5,
                    plasma_orbs: [0x9155, 0x9175, 0x9195, 0x91b5],
                    bob_table: 0x8605,
                    barrel_table: 0xf521,
                },
                sfx: SfxData {
                    name_table: 0x8246,
                    sample_lengths: L7_SFX_LENGTHS,
                },
                music_track: 6,
                // Both layers share row 1 and rate 16 on separate
                // accumulators; the split is back-vs-front art, not depth.
                scenery: SceneryData {
                    cs_base: 0x51e0,
                    cell_base: 273,
                    layers: &[
                        SceneryLayerData {
                            cs_offset: 0x39cd,
                            top: 1,
                            speed: 16,
                        },
                        SceneryLayerData {
                            cs_offset: 0x3c2f,
                            top: 1,
                            speed: 16,
                        },
                    ],
                    front_layers: 0,
                },
                stars: &[],
                camera_min: 0,
                spawns: SpawnSource::Generated {
                    script: level_7::script,
                    post_pass: Some(level_7::post_pass),
                },
                spawn_positions: Some(SpawnPositionsData {
                    table: 0x8635,
                    rows: 136,
                    ai: None,
                }),
            },
        }
    }
}

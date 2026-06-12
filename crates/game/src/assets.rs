//! Loading game assets from the original disc.
//!
//! The front-end reads its graphics from the CD: `BACK3.RAW` (the menu
//! background), `FONT.RAW` (the glyph sheet) and the menu palette baked into
//! `START.EXE`. This module turns those raw bytes into decoded values the
//! scenes and the audio backend consume. It depends on the disc reader but on
//! nothing graphical and nothing audio-device specific.

use anyhow::{Context, Result};
use prototype_disc::{AssetSource, DiscImage};
use prototype_formats::bin::{SpriteSheet, decode_banked, decode_banked_direct, decode_ship};
use prototype_formats::font::Font;
use prototype_formats::{Dimensions, Flic, IndexedImage, Palette, StartExe, bdy, pal, raw, wad};

use std::sync::Arc;

use crate::background::{Background, Sp};
use crate::level::spawn::SpawnSource;
use crate::levels::{Level, Overlay, SceneryData, SfxData, ShipData, SpawnAi, StarPlaneData};
use crate::scenery::{Scenery, SceneryLayer};
use crate::screen::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::sfx::SfxBank;
use crate::ship::SHIELD_FRAMES;
use crate::spawns::SpawnRow;
use openprototype_core::PerWeapon;

/// Everything the main menu needs to render.
pub struct MenuAssets {
    pub background: IndexedImage,
    pub font: Font,
    pub palette: Palette,
}

/// A full-screen still with its own palette (a `.BDY` image plus its `.PAL`).
pub struct StillImage {
    pub image: IndexedImage,
    pub palette: Palette,
}

/// Everything the intro sequence needs. The stills are decoded up front; the
/// FLIs are kept as raw bytes and decoded when their beat starts (each is large,
/// and the intro plays once). Their headers are validated here so gross
/// corruption surfaces at load, not mid-intro.
pub struct IntroAssets {
    pub neo: StillImage,
    pub surplogo: StillImage,
    pub cover: StillImage,
    pub intro_fli: Vec<u8>,
    pub fly_fli: Vec<u8>,
    pub credz_fli: Vec<u8>,
    pub font: Font,
}

/// What the high-score screen needs: the `HIGHSCOR.FLI` backdrop (kept as bytes,
/// decoded when the scene starts) and the second font the original draws the
/// entries with. The table itself comes from the [`HighscoreStore`], loaded when
/// the scene is built.
///
/// [`HighscoreStore`]: crate::highscores::HighscoreStore
pub struct HighscoreAssets {
    pub fli: Vec<u8>,
    pub font: Font,
}

/// `COVER3.BDY` decodes to a 320x478 image (taller than the screen); the intro
/// shows the top 320x200, where the PROTOTYPE title and ship sit.
const COVER_HEIGHT: u32 = 478;

/// Load and decode the menu assets from the disc image.
pub fn load_menu_assets(disc: &DiscImage) -> Result<MenuAssets> {
    let background_bytes = disc.read("BACK3.RAW").context("reading BACK3.RAW")?;
    let background = raw::decode(
        &background_bytes,
        Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
    )
    .context("decoding BACK3.RAW")?;

    let font_bytes = disc.read("FONT.RAW").context("reading FONT.RAW")?;
    let font = Font::decode(&font_bytes).context("decoding FONT.RAW")?;

    let start_exe_bytes = disc.read("START.EXE").context("reading START.EXE")?;
    let palette = StartExe::new(&start_exe_bytes)
        .context("parsing START.EXE")?
        .menu_palette()
        .context("decoding menu palette")?;

    Ok(MenuAssets {
        background,
        font,
        palette,
    })
}

/// Everything the in-game HUD needs to render.
///
/// The HUD shares the level's embedded palette (read from the `.WAD`); the
/// digit sheets are columns of stacked glyphs, sliced per digit at draw time.
pub struct HudAssets {
    pub palette: Palette,
    /// The panel background, 320x32.
    pub panel: IndexedImage,
    /// The score readout's ten LCD numerals 0..=9, 16x13 each, stacked.
    pub score_digits: IndexedImage,
    /// The count numerals 1..=9 (lives and similar), 12x10 each, stacked.
    pub number_digits: IndexedImage,
    /// The four weapon charge bars as 64-wide gradient rows (4 rows per weapon);
    /// a 32-px window into each slides right with the weapon's level.
    pub weapon_bars: IndexedImage,
    /// The smart-bomb indicator's four frames (counts 0..=3), 40x9 each, stacked.
    pub smart_frames: IndexedImage,
    /// The weapon-selector lights, 12 wide: a 28-row base of four unselected
    /// slots, then four 7-row highlights, one per selected slot.
    pub selector_lights: IndexedImage,
    /// The weapon pods drawn in the panel's right recess: 5 weapons (columns) by
    /// 6 animation frames (rows), 56x32 each; the bottom row is the settled
    /// state. EXTRAS.RAW, 280x192.
    pub weapon_pods: IndexedImage,
}

/// Everything the in-game level scene needs to render: the level's scrolling
/// canyon background (decoded to one still here), the HUD, and the weapon-top
/// overlay sprites.
pub struct LevelAssets {
    /// The level's parallax background: the SP image (de-interleaved from the
    /// four `.SPn` planes to one 640x160 still) plus its strip layout. The image
    /// is wider than the screen; the level scrolls a window across it.
    pub background: Background,
    pub hud: HudAssets,
    /// Each weapon's weapon-top overlay that clips over the panel: the firing
    /// weapon's cut-off top. The chaingun has none.
    pub overlays: PerWeapon<OverlaySprite>,
    /// Each weapon's per-frame overlay slide as it settles, `(dx, dy)` relative
    /// to its settled position, indexed by animation frame. Each weapon has its
    /// own profile (some snap, some kick sideways).
    pub overlay_slide: PerWeapon<[(i32, i32); OVERLAY_FRAMES]>,
    /// The decoded sprite catalog the scenery layers index by cell, read with
    /// the walker's direct plane addressing (the header-preferring sheet the
    /// overlays use would corrupt cells whose code bytes also parse as a
    /// clip-header).
    pub catalog: SpriteSheet,
    /// The level's parallax scenery layers, decoded from the level WAD's tilemaps.
    pub scenery: Scenery,
    /// The ship's frames from `PTURN1.BN1`: the 27-frame barrel-roll cycle
    /// plus the level's idle exhaust-flicker frame(s) (see [`crate::ship`]).
    pub ship_frames: SpriteSheet,
    /// The level's ship frame selection, straight from the registry.
    pub ship: ShipData,
    /// The shield's animation frames, assembled from the WAD's sprite
    /// directory over the clip-header catalog.
    pub shield_frames: Vec<OverlaySprite>,
    /// The player-fire sprites (shots and muzzle flash).
    pub fire_sprites: FireSprites,
    /// Per roll frame, the ship's two barrel y offsets (shot spawns and the
    /// muzzle flash both anchor on them).
    pub barrel_offsets: Vec<(i32, i32)>,
    /// The plasma orbs' bob wave, sampled at staggered phases per orb.
    pub bob_wave: Vec<i32>,
    /// The smart-bomb ring's 32 `(vx, vy)` velocities (12.4), around a circle.
    pub bomb_wave: Vec<(i32, i32)>,
    /// The level's sound-effect samples, indexed by slot.
    pub sfx: SfxBank,
    /// The level's CD-DA music track and its loop period.
    pub music: LevelMusic,
    /// The level's star-field planes, straight from the registry (the positions
    /// are generated per scene, not loaded).
    pub stars: &'static [StarPlaneData],
    /// The vertical camera range start (and starting row), from the registry.
    pub camera_min: i32,
    /// The raw WAD image, kept for runtime descriptor reads (the spawn layer
    /// resolves sprite descriptors by cs-pointer as entities animate).
    pub wad: Vec<u8>,
    /// The clip-header reading of the catalog (the one the shield/fire
    /// directory records use); the spawn sprites assemble from it.
    pub clip_catalog: SpriteSheet,
    /// The WAD's cs-to-file offset (`file = cs + cs_base`), for descriptor
    /// pointer resolution.
    pub cs_base: usize,
    /// The level's spawn-position table rows, `None` until that level's table
    /// is reverse-engineered.
    pub spawn_rows: Option<Vec<SpawnRow>>,
    /// The level's spawn source, straight from the registry.
    pub spawns: SpawnSource,
    /// The level's transcribed AI set, if any.
    pub spawn_ai: Option<SpawnAi>,
    /// The level's combat constants, straight from the registry.
    pub combat: crate::levels::CombatData,
    /// The glyph sheet (`FONT.RAW`), for the GET READY overlay.
    pub font: Font,
    /// Per palette index, the index of the nearest color to one third of its
    /// brightness; the GET READY freeze remaps the playfield through it.
    pub dim_table: [u8; 256],
    /// The ship's death-explosion frames, decoded from
    /// [`ShipData::explosion`]; empty until that level's descriptors are
    /// found.
    pub ship_explosion: Vec<OverlaySprite>,
}

/// A masked sprite: `None` is transparent. Used for the weapon overlay, which
/// the original draws over the playfield/panel without a color key.
pub struct OverlaySprite {
    pub size: Dimensions,
    pub pixels: Vec<Option<u8>>,
}

/// The player-fire sprites, assembled from the WAD's directory records over
/// the clip-header catalog (see [`crate::shots`]).
pub struct FireSprites {
    pub chaingun: OverlaySprite,
    /// Per charge level 1..=4.
    pub multishot: [OverlaySprite; 4],
    pub burning: [OverlaySprite; 4],
    pub plasma_bolt: OverlaySprite,
    /// The missile per facing octant (`0` = right, counting clockwise): 8
    /// consecutive directory records, steering picks the frame.
    pub missile: [OverlaySprite; 8],
    /// The smart-bomb ring shot (per level; only L1's aliases the multishot).
    pub bomb_ring: OverlaySprite,
    /// The chaingun muzzle flash's 6 animation frames.
    pub muzzle_flash: Vec<OverlaySprite>,
    /// The four plasma orbs, 4 animation frames each.
    pub plasma_orbs: [[OverlaySprite; 4]; 4],
}

/// Frames in a weapon's pod/overlay open-and-settle animation (`0` hidden ..=
/// `5` settled).
pub const OVERLAY_FRAMES: usize = 6;

/// Width of one Mode X catalog cell, in pixels.
const CELL_WIDTH: usize = 32;

/// Positions stored per weapon block; frames 6 and 7 are unused padding.
const OVERLAY_BLOCK_FRAMES: usize = 8;
/// Frame index of the settled position, which the slide deltas are relative to.
const OVERLAY_SETTLED_FRAME: usize = 5;

/// One Mode X plane: 160 bytes per row (every fourth column of a 640-wide row),
/// 160 rows. The four `.SPn` files together make a 640x160 image: a canyon wider
/// than the screen, scrolled horizontally, the playfield's 160 rows tall.
const BACKGROUND_SIZE: Dimensions = Dimensions {
    width: 640,
    height: 160,
};
const SP_PLANE_STRIDE: usize = 160;
const SP_PLANE_LEN: usize = SP_PLANE_STRIDE * 160;

const PANEL_SIZE: Dimensions = Dimensions {
    width: 320,
    height: 32,
};
const SCORE_DIGITS_SIZE: Dimensions = Dimensions {
    width: 16,
    height: 130,
};
const NUMBER_DIGITS_SIZE: Dimensions = Dimensions {
    width: 12,
    height: 90,
};
const WEAPON_BARS_SIZE: Dimensions = Dimensions {
    width: 64,
    height: 16,
};
const SMART_FRAMES_SIZE: Dimensions = Dimensions {
    width: 40,
    height: 36,
};
const SELECTOR_LIGHTS_SIZE: Dimensions = Dimensions {
    width: 12,
    height: 56,
};
const WEAPON_PODS_SIZE: Dimensions = Dimensions {
    width: 280,
    height: 192,
};

/// Load and decode the in-game HUD assets from the disc image. The HUD palette
/// is the playing level's, read from `palette_wad`.
pub fn load_hud_assets(disc: &DiscImage, palette_wad: &str) -> Result<HudAssets> {
    let wad_bytes = disc
        .read(palette_wad)
        .with_context(|| format!("reading {palette_wad}"))?;
    let palette = wad::level_palette(&wad_bytes).context("extracting the level palette")?;

    let panel = decode_raw(disc, "PANEL.RAW", PANEL_SIZE)?;
    let score_digits = decode_raw(disc, "SCORE.RAW", SCORE_DIGITS_SIZE)?;
    let number_digits = decode_raw(disc, "NUMBERS.RAW", NUMBER_DIGITS_SIZE)?;
    let weapon_bars = decode_raw(disc, "BALKEN.RAW", WEAPON_BARS_SIZE)?;
    let smart_frames = decode_raw(disc, "SMART.RAW", SMART_FRAMES_SIZE)?;
    let selector_lights = decode_raw(disc, "LIGHTS.RAW", SELECTOR_LIGHTS_SIZE)?;
    let weapon_pods = decode_raw(disc, "EXTRAS.RAW", WEAPON_PODS_SIZE)?;

    Ok(HudAssets {
        palette,
        panel,
        score_digits,
        number_digits,
        weapon_bars,
        smart_frames,
        selector_lights,
        weapon_pods,
    })
}

/// The death sequence steps the explosion descriptor offset by 8 until it
/// reaches `0xb8` (`cs:0x8f5f` in the tick at file `0xb354`): 23 frames.
const SHIP_EXPLOSION_FRAMES: usize = 23;

/// Build the playfield-dimming remap (the level init's table builder at file
/// `0xe4be`): for each palette index, the target is its color divided by the
/// level's brightness divisor (6-bit channels, integer division), and the
/// entry is the palette index nearest that target by L1 distance, earliest
/// index winning ties. The GET READY freeze (file `0xe60f`) remaps every
/// playfield pixel through the table.
fn darken_table(palette: &Palette, divisor: i32) -> [u8; 256] {
    // The palette stores bit-replicated 8-bit channels; `>> 2` recovers the
    // 6-bit DAC values the original works in.
    let dac: Vec<[i32; 3]> = palette
        .colors
        .iter()
        .map(|color| {
            [
                i32::from(color.r >> 2),
                i32::from(color.g >> 2),
                i32::from(color.b >> 2),
            ]
        })
        .collect();

    std::array::from_fn(|index| {
        let target = dac[index].map(|channel| channel / divisor);
        let mut best_distance = i32::MAX;
        let mut best_index = 0u8;

        for (candidate, color) in dac.iter().enumerate() {
            let distance = (color[0] - target[0]).abs()
                + (color[1] - target[1]).abs()
                + (color[2] - target[2]).abs();

            if distance < best_distance {
                best_distance = distance;
                best_index = candidate as u8;
            }
        }

        best_index
    })
}

/// Load and decode the level scene's assets: the parallax background, the HUD,
/// and the weapon overlays with their per-weapon slide tables.
pub fn load_level_assets(disc: &DiscImage, level: Level) -> Result<LevelAssets> {
    let data = level.data();

    let background = Background::new(load_background(disc, data.background)?, data.background);
    let hud = load_hud_assets(disc, data.wad)?;

    let bin_name = format!("{}.BIN", data.catalog.stem());
    let bin = disc
        .read(&bin_name)
        .with_context(|| format!("reading {bin_name}"))?;
    let wad = disc
        .read(data.wad)
        .with_context(|| format!("reading {}", data.wad))?;
    // The weapon overlays are clip-header records; the scenery cells are
    // direct subroutines. The two readings disagree on ambiguous bytes, so
    // each consumer decodes the catalog its own way.
    let mut overlay_catalog = decode_banked(&bin, &wad, data.catalog_offset)
        .with_context(|| format!("decoding {bin_name}"))?;
    let overlays = load_overlays(&overlay_catalog, data.overlays)?;

    // The sprite descriptors' cell numbering starts at the level's entity
    // cell base (nonzero in the race catalogs); dropping the prefix makes
    // every descriptor's cell field index the sheet directly.
    overlay_catalog.sprites.drain(..data.entity_cell_base);
    let catalog = decode_banked_direct(&bin, &wad, data.catalog_offset)
        .with_context(|| format!("decoding {bin_name} for scenery"))?;
    let overlay_slide = read_overlay_slide(&wad, data.overlay_positions)?;
    let scenery = decode_scenery(&wad, data.scenery);

    let pturn1 = disc.read("PTURN1.BN1").context("reading PTURN1.BN1")?;
    let ship_frames = decode_ship(&pturn1, &wad, data.ship.catalog).context("decoding PTURN1")?;
    let shield_frames = load_shield_frames(&wad, data.shield_directory, &overlay_catalog)?;
    let (fire_sprites, barrel_offsets, bob_wave) = load_fire(&wad, &overlay_catalog, data.fire)?;
    let sfx = load_sfx(disc, &wad, data.sfx)?;
    let music = load_music(disc, data.music_track)?;
    let font_bytes = disc.read("FONT.RAW").context("reading FONT.RAW")?;
    let font = Font::decode(&font_bytes).context("decoding FONT.RAW")?;
    let dim_table = darken_table(&hud.palette, data.dim_divisor);
    let bomb_wave = read_bomb_wave(&wad, data.fire.bomb_wave)?;
    let ship_explosion = data
        .ship
        .explosion
        .map(|base| {
            (0..SHIP_EXPLOSION_FRAMES)
                .map(|frame| directory_sprite(&wad, &overlay_catalog, base + frame * 8))
                .collect::<Result<Vec<_>>>()
        })
        .transpose()
        .context("decoding the ship explosion frames")?
        .unwrap_or_default();
    let spawn_rows = data
        .spawn_positions
        .map(|positions| crate::spawns::decode_rows(&wad, positions.table, positions.rows))
        .transpose()
        .context("decoding the spawn-position table")?;

    Ok(LevelAssets {
        background,
        hud,
        overlays,
        overlay_slide,
        catalog,
        scenery,
        ship_frames,
        ship: data.ship,
        shield_frames,
        fire_sprites,
        barrel_offsets,
        bob_wave,
        sfx,
        music,
        stars: data.stars,
        camera_min: data.camera_min,
        wad,
        clip_catalog: overlay_catalog,
        cs_base: data.scenery.cs_base,
        spawn_rows,
        spawns: data.spawns,
        spawn_ai: data.spawn_positions.and_then(|positions| positions.ai),
        combat: data.combat,
        font,
        dim_table,
        ship_explosion,
        bomb_wave,
    })
}

/// Read the smart-bomb ring's 32 `(vx, vy)` 12.4 velocity pairs.
fn read_bomb_wave(wad: &[u8], table: usize) -> Result<Vec<(i32, i32)>> {
    let end = table + 32 * 4;

    if wad.len() < end {
        anyhow::bail!("bomb-wave table out of bounds");
    }

    Ok(wad[table..end]
        .chunks_exact(4)
        .map(|pair| {
            (
                i32::from(i16::from_le_bytes([pair[0], pair[1]])),
                i32::from(i16::from_le_bytes([pair[2], pair[3]])),
            )
        })
        .collect())
}

/// A level's music: which CD-DA track it plays and the track's TOC length in
/// logic ticks. The original starts its track once at level begin and loops
/// it by timer: the length counts down in the 60 Hz timer ISR and an
/// underflow restarts the track (the driver recomputes the same length from
/// the TOC each time).
pub struct LevelMusic {
    pub track: u8,
    pub length_ticks: u32,
}

/// CD-DA frames (sectors) per second.
const CD_FRAMES_PER_SECOND: u32 = 75;

/// Logic ticks per second (the level's timer ISR rate).
const TICKS_PER_SECOND: u32 = 60;

/// Look up the level's music track and compute its loop period the way the
/// original's driver does: the TOC track length in frames, floored to whole
/// seconds, times 60. The TOC length runs to the NEXT track's start (so it
/// includes the next track's 2-second pregap); the last track runs to the
/// disc's end.
fn load_music(disc: &DiscImage, track: u8) -> Result<LevelMusic> {
    let tracks = disc.audio_tracks();
    let position = tracks
        .iter()
        .position(|candidate| candidate.number == track)
        .with_context(|| format!("disc has no audio track {track}"))?;

    let start = tracks[position].start_lba;
    let end = match tracks.get(position + 1) {
        Some(next) => next.start_lba,
        None => tracks[position].end_lba,
    };

    Ok(LevelMusic {
        track,
        length_ticks: (end - start) / CD_FRAMES_PER_SECOND * TICKS_PER_SECOND,
    })
}

/// Bytes per entry in the WAD's `.SMP` filename table.
const SFX_NAME_STRIDE: usize = 16;

/// Load the level's sound-effect samples: read the WAD's NUL-padded filename
/// table and pull each `.SMP` off the disc, cut to its trigger's authored
/// length (see [`SfxData`]). The files are raw signed 8-bit mono at 11111 Hz
/// and are kept that way; the platform's mixer does the format conversion.
fn load_sfx(disc: &DiscImage, wad: &[u8], data: SfxData) -> Result<SfxBank> {
    let table_end = data.name_table + data.sample_lengths.len() * SFX_NAME_STRIDE;

    if wad.len() < table_end {
        anyhow::bail!(
            "WAD is {} bytes, the SMP name table needs {table_end}",
            wad.len()
        );
    }

    let samples = data
        .sample_lengths
        .iter()
        .enumerate()
        .map(|(slot, &length)| {
            let entry_start = data.name_table + slot * SFX_NAME_STRIDE;
            let entry = &wad[entry_start..entry_start + SFX_NAME_STRIDE];
            let name_length = entry
                .iter()
                .position(|&byte| byte == 0)
                .unwrap_or(entry.len());
            let name = std::str::from_utf8(&entry[..name_length])
                .with_context(|| format!("SMP name table slot {slot} is not text"))?
                .to_ascii_uppercase();

            let bytes = disc
                .read(&name)
                .with_context(|| format!("reading {name}"))?;
            let cut = bytes[..length.min(bytes.len())]
                .iter()
                .map(|&byte| byte as i8)
                .collect::<Vec<i8>>();

            Ok(Arc::from(cut))
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(SfxBank { samples })
}

/// Assemble one sprite from a directory record at `record` in the WAD:
/// `{ncells, width, height, cell}` (all `u16`), `cell` indexing the level's
/// clip-header catalog.
pub(crate) fn directory_sprite(
    wad: &[u8],
    catalog: &SpriteSheet,
    record: usize,
) -> Result<OverlaySprite> {
    if wad.len() < record + 8 {
        anyhow::bail!(
            "WAD is {} bytes, directory record at {record} overruns",
            wad.len()
        );
    }

    let read = |at: usize| usize::from(u16::from_le_bytes([wad[at], wad[at + 1]]));
    let ncells = read(record);
    let cell = read(record + 6);

    if catalog.sprites.len() < cell + ncells {
        anyhow::bail!(
            "catalog has {} sprites, directory record at {record} needs {}",
            catalog.sprites.len(),
            cell + ncells
        );
    }

    Ok(assemble_overlay(catalog, cell, ncells))
}

/// What [`load_fire`] produces: the fire sprites, the per-roll-frame barrel
/// pairs, and the orbs' bob wave.
type FireAssets = (FireSprites, Vec<(i32, i32)>, Vec<i32>);

/// Load the player-fire sprites, the barrel-offset table, and the orbs' bob
/// wave.
fn load_fire(
    wad: &[u8],
    catalog: &SpriteSheet,
    fire: crate::levels::FireData,
) -> Result<FireAssets> {
    let sprite = |record: usize| directory_sprite(wad, catalog, record);
    let leveled = |records: [usize; 4]| -> Result<[OverlaySprite; 4]> {
        Ok([
            sprite(records[0])?,
            sprite(records[1])?,
            sprite(records[2])?,
            sprite(records[3])?,
        ])
    };

    let muzzle_flash = (0..6)
        .map(|frame| sprite(fire.muzzle_flash + frame * 8))
        .collect::<Result<Vec<_>>>()?;

    let table_end = fire.barrel_table + ROLL_FRAMES * 4;

    if wad.len() < table_end {
        anyhow::bail!(
            "WAD is {} bytes, the barrel table needs {table_end}",
            wad.len()
        );
    }

    let barrels = (0..ROLL_FRAMES)
        .map(|frame| {
            let at = fire.barrel_table + frame * 4;
            let pair = |offset: usize| {
                i32::from(i16::from_le_bytes([wad[at + offset], wad[at + offset + 1]]))
            };

            (pair(0), pair(2))
        })
        .collect();

    let wave_end = fire.bob_table + BOB_WAVE_WORDS * 2;

    if wad.len() < wave_end {
        anyhow::bail!("WAD is {} bytes, the bob wave needs {wave_end}", wad.len());
    }

    let bob_wave = (0..BOB_WAVE_WORDS)
        .map(|word| {
            let at = fire.bob_table + word * 2;

            i32::from(i16::from_le_bytes([wad[at], wad[at + 1]]))
        })
        .collect();

    let orb = |base: usize| -> Result<[OverlaySprite; 4]> {
        Ok([
            sprite(base)?,
            sprite(base + 8)?,
            sprite(base + 16)?,
            sprite(base + 24)?,
        ])
    };

    Ok((
        FireSprites {
            chaingun: sprite(fire.chaingun)?,
            multishot: leveled(fire.multishot)?,
            burning: leveled(fire.burning)?,
            plasma_bolt: sprite(fire.plasma_bolt)?,
            missile: [
                sprite(fire.missile)?,
                sprite(fire.missile + 8)?,
                sprite(fire.missile + 16)?,
                sprite(fire.missile + 24)?,
                sprite(fire.missile + 32)?,
                sprite(fire.missile + 40)?,
                sprite(fire.missile + 48)?,
                sprite(fire.missile + 56)?,
            ],
            bomb_ring: sprite(fire.bomb_sprite)?,
            muzzle_flash,
            plasma_orbs: [
                orb(fire.plasma_orbs[0])?,
                orb(fire.plasma_orbs[1])?,
                orb(fire.plasma_orbs[2])?,
                orb(fire.plasma_orbs[3])?,
            ],
        },
        barrels,
        bob_wave,
    ))
}

/// Words read from the orbs' bob wave: enough to cover the largest phase
/// stagger (10 bytes) plus the 28-byte phase range.
const BOB_WAVE_WORDS: usize = 20;

/// Frames in the ship's barrel-roll cycle (the barrel table has one pair per
/// frame).
const ROLL_FRAMES: usize = 27;

/// Assemble the shield's animation frames from the WAD's sprite directory.
///
/// Each 8-byte record is `{ncells, width, height, cell}` (all `u16`), `cell`
/// indexing the level's clip-header catalog; the animation cycles the first
/// [`SHIELD_FRAMES`] records.
fn load_shield_frames(
    wad: &[u8],
    directory: usize,
    catalog: &SpriteSheet,
) -> Result<Vec<OverlaySprite>> {
    let table_end = directory + SHIELD_FRAMES * 8;

    if wad.len() < table_end {
        anyhow::bail!(
            "WAD is {} bytes, the shield directory needs {table_end}",
            wad.len()
        );
    }

    (0..SHIELD_FRAMES)
        .map(|frame| {
            let record = directory + frame * 8;
            let read = |at: usize| usize::from(u16::from_le_bytes([wad[at], wad[at + 1]]));
            let ncells = read(record);
            let cell = read(record + 6);

            if catalog.sprites.len() < cell + ncells {
                anyhow::bail!(
                    "catalog has {} sprites, shield frame {frame} needs {}",
                    catalog.sprites.len(),
                    cell + ncells
                );
            }

            Ok(assemble_overlay(catalog, cell, ncells))
        })
        .collect()
}

/// Assemble each weapon's overlay from the decoded catalog.
fn load_overlays(
    catalog: &SpriteSheet,
    overlays: PerWeapon<Overlay>,
) -> Result<PerWeapon<OverlaySprite>> {
    let needed = [
        overlays.multishot,
        overlays.burning,
        overlays.plasma,
        overlays.missile,
    ]
    .iter()
    .map(|overlay| overlay.first + overlay.count)
    .max()
    .unwrap_or(0);

    if catalog.sprites.len() < needed {
        anyhow::bail!(
            "catalog has {} sprites, the overlays need {needed}",
            catalog.sprites.len()
        );
    }

    Ok(overlays.map(|overlay| assemble_overlay(catalog, overlay.first, overlay.count)))
}

/// Decode a level's parallax scenery layers from its WAD.
///
/// Each layer is a tilemap of catalog-cell codes at a fixed WAD offset (the
/// faithful engine points `cs:0x31c4` at one per layer and walks it by the scroll
/// column). Layers are drawn back to front; the front one sits over the playfield
/// in the original, so it draws after the ship. A level whose scenery is not yet
/// reverse-engineered has no layers and yields an empty [`Scenery`].
fn decode_scenery(wad: &[u8], scenery: SceneryData) -> Scenery {
    let layers = scenery
        .layers
        .iter()
        .map(|layer| {
            SceneryLayer::new(
                decode_scenery_tilemap(wad, scenery.cs_base, scenery.cell_base, layer.cs_offset),
                layer.top,
                layer.speed,
            )
        })
        .collect();

    Scenery::new(layers, scenery.front_layers)
}

/// Expand one scenery tilemap into a per-column `Some(catalog cell)` / `None`
/// strip, exactly one loop long. The stream is bytes: `0` is an empty column,
/// `0xFF` is a jump to the 16-bit cs-offset that follows, and any other byte `n`
/// is catalog cell `n + cell_base` (the per-level offset the render routine bakes
/// in; L1 `-1`, the shooter levels `273`, the race levels `968`/`978`/`1106`).
///
/// Each layer's stream ends in a jump back to its own start, so the strip is a
/// short repeating pattern (the original loops it under the level forever).
/// Following the stream until an offset repeats yields one clean loop, which the
/// layer then wraps at its true period rather than at an arbitrary cut.
fn decode_scenery_tilemap(
    wad: &[u8],
    cs_base: usize,
    cell_base: i32,
    start: usize,
) -> Vec<Option<usize>> {
    let mut tiles = Vec::new();
    let mut visited = std::collections::HashSet::new();
    let mut cs = start;

    while visited.insert(cs) {
        let file = cs + cs_base;

        if file + 2 >= wad.len() {
            break;
        }

        match wad[file] {
            0xFF => cs = usize::from(u16::from_le_bytes([wad[file + 1], wad[file + 2]])),
            0 => {
                tiles.push(None);
                cs += 1;
            }
            code => {
                let cell = i32::from(code) + cell_base;
                tiles.push(usize::try_from(cell).ok());
                cs += 1;
            }
        }
    }

    tiles
}

/// Read each weapon's overlay slide from the WAD's position table at `table`
/// (per-level, [`LevelData::overlay_positions`]), as `(dx, dy)` per frame
/// relative to the settled frame. Each weapon's profile differs, so this is
/// read rather than assumed. Per weapon: a block of [`OVERLAY_BLOCK_FRAMES`]
/// `(x, y)` `u16` positions; the animation only uses the first
/// [`OVERLAY_FRAMES`].
fn read_overlay_slide(wad: &[u8], table: usize) -> Result<PerWeapon<[(i32, i32); OVERLAY_FRAMES]>> {
    // The WAD table holds one block per firing weapon: block 0 is the chaingun
    // (which has no overlay), then the four real weapons in selector order.
    const TABLE_BLOCKS: usize = 5;
    let table_end = table + TABLE_BLOCKS * OVERLAY_BLOCK_FRAMES * 4;

    if wad.len() < table_end {
        anyhow::bail!(
            "WAD is {} bytes, the overlay position table needs {table_end}",
            wad.len()
        );
    }

    let read_xy = |offset: usize| -> (i32, i32) {
        let x = u16::from_le_bytes([wad[offset], wad[offset + 1]]);
        let y = u16::from_le_bytes([wad[offset + 2], wad[offset + 3]]);
        (i32::from(x), i32::from(y))
    };

    let slide_block = |table_index: usize| -> [(i32, i32); OVERLAY_FRAMES] {
        let block = table + table_index * OVERLAY_BLOCK_FRAMES * 4;
        let (settled_x, settled_y) = read_xy(block + OVERLAY_SETTLED_FRAME * 4);

        std::array::from_fn(|frame| {
            let (x, y) = read_xy(block + frame * 4);
            (x - settled_x, y - settled_y)
        })
    };

    Ok(PerWeapon {
        multishot: slide_block(1),
        burning: slide_block(2),
        plasma: slide_block(3),
        missile: slide_block(4),
    })
}

/// Stitch a multi-cell overlay into one masked sprite. Cell `k` occupies screen
/// columns `[k*32, k*32+32)`; within it, the decoded sprite sits at its trimmed
/// `origin`, so each cell's pixels land at `(k*32 + origin.x, origin.y)`.
fn assemble_overlay(sheet: &SpriteSheet, first: usize, cells: usize) -> OverlaySprite {
    let mut width = 0usize;
    let mut height = 0usize;

    for cell in 0..cells {
        let sprite = &sheet.sprites[first + cell];
        let origin_x = cell * CELL_WIDTH + sprite.origin.0.max(0) as usize;
        let origin_y = sprite.origin.1.max(0) as usize;
        width = width.max(origin_x + sprite.size.width as usize);
        height = height.max(origin_y + sprite.size.height as usize);
    }

    let mut pixels = vec![None; width * height];

    for cell in 0..cells {
        let sprite = &sheet.sprites[first + cell];
        let sprite_width = sprite.size.width as usize;
        let origin_x = cell * CELL_WIDTH + sprite.origin.0.max(0) as usize;
        let origin_y = sprite.origin.1.max(0) as usize;

        for sy in 0..sprite.size.height as usize {
            for sx in 0..sprite_width {
                if let Some(value) = sprite.pixels[sy * sprite_width + sx] {
                    pixels[(origin_y + sy) * width + origin_x + sx] = Some(value);
                }
            }
        }
    }

    OverlaySprite {
        size: Dimensions::new(width as u32, height as u32),
        pixels,
    }
}

/// De-interleave an SP background's four `.SPn` planes into one 640x160 still.
///
/// Each `.SPn` file is one Mode X plane holding every fourth column, so pixel
/// `(x, y)` lives in plane `x % 4` at byte `y * 160 + x / 4`.
fn load_background(disc: &DiscImage, sp: Sp) -> Result<IndexedImage> {
    let mut planes = Vec::with_capacity(4);

    for index in 1..=4 {
        let name = format!("{}.SP{index}", sp.stem());
        let bytes = disc
            .read(&name)
            .with_context(|| format!("reading {name}"))?;

        if bytes.len() < SP_PLANE_LEN {
            anyhow::bail!("{name} is {} bytes, expected {SP_PLANE_LEN}", bytes.len());
        }

        planes.push(bytes);
    }

    let width = BACKGROUND_SIZE.width as usize;
    let height = BACKGROUND_SIZE.height as usize;
    let mut pixels = vec![0u8; width * height];

    for y in 0..height {
        for x in 0..width {
            pixels[y * width + x] = planes[x & 3][y * SP_PLANE_STRIDE + (x >> 2)];
        }
    }

    Ok(IndexedImage::new(BACKGROUND_SIZE, pixels).expect("canyon still matches its dimensions"))
}

/// Read and decode a linear `.RAW` graphic of known dimensions from the disc.
fn decode_raw(disc: &DiscImage, name: &str, size: Dimensions) -> Result<IndexedImage> {
    let bytes = disc.read(name).with_context(|| format!("reading {name}"))?;
    raw::decode(&bytes, size).with_context(|| format!("decoding {name}"))
}

/// Load and decode the intro assets from the disc image.
pub fn load_intro_assets(disc: &DiscImage) -> Result<IntroAssets> {
    let neo = load_still(disc, "NEO.BDY", "NEO.PAL")?;
    let surplogo = load_still(disc, "SURPLOGO.BDY", "SURPLOGO.PAL")?;
    let cover = load_cover(disc)?;

    let intro_fli = load_fli_bytes(disc, "FLI/INTRO.FLI")?;
    let fly_fli = load_fli_bytes(disc, "FLI/FLY.FLI")?;
    let credz_fli = load_fli_bytes(disc, "FLI/CREDZ.FLI")?;

    let font_bytes = disc.read("FONT.RAW").context("reading FONT.RAW")?;
    let font = Font::decode(&font_bytes).context("decoding FONT.RAW")?;

    Ok(IntroAssets {
        neo,
        surplogo,
        cover,
        intro_fli,
        fly_fli,
        credz_fli,
        font,
    })
}

/// Decode a full-screen `.BDY` still and its `.PAL` palette.
fn load_still(disc: &DiscImage, body: &str, palette: &str) -> Result<StillImage> {
    let body_bytes = disc.read(body).with_context(|| format!("reading {body}"))?;
    let image = bdy::decode(&body_bytes, Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT))
        .with_context(|| format!("decoding {body}"))?;

    let palette_bytes = disc
        .read(palette)
        .with_context(|| format!("reading {palette}"))?;
    let palette = pal::decode(&palette_bytes).with_context(|| format!("decoding {palette}"))?;

    Ok(StillImage { image, palette })
}

/// Rows of the cover the original puts on screen: it copies 0x7d00 bytes per
/// plane into an unchained 400-line tweak of mode 13h, the top 400 rows of the
/// 478-row BDY.
const COVER_VISIBLE_HEIGHT: u32 = 400;

/// Decode the PROTOTYPE cover at the original's display size. `COVER3.BDY` is
/// a 320x478 image shown as 320x400 (the renderer's 4:3 fit squashes it to the
/// same shape the CRT did); the intro swaps its framebuffer to this size for
/// the cover beats.
fn load_cover(disc: &DiscImage) -> Result<StillImage> {
    let body_bytes = disc.read("COVER3.BDY").context("reading COVER3.BDY")?;
    let full = bdy::decode(&body_bytes, Dimensions::new(SCREEN_WIDTH, COVER_HEIGHT))
        .context("decoding COVER3.BDY")?;

    let visible = (SCREEN_WIDTH * COVER_VISIBLE_HEIGHT) as usize;
    let image = IndexedImage::new(
        Dimensions::new(SCREEN_WIDTH, COVER_VISIBLE_HEIGHT),
        full.pixels[..visible].to_vec(),
    )
    .expect("cropped cover matches its dimensions");

    let palette_bytes = disc.read("COVER3.PAL").context("reading COVER3.PAL")?;
    let palette = pal::decode(&palette_bytes).context("decoding COVER3.PAL")?;

    Ok(StillImage { image, palette })
}

/// Read a FLI's bytes and validate its header, so a corrupt file fails at load
/// rather than when its beat plays.
fn load_fli_bytes(disc: &DiscImage, name: &str) -> Result<Vec<u8>> {
    let bytes = disc.read(name).with_context(|| format!("reading {name}"))?;
    Flic::new(&bytes).with_context(|| format!("validating {name} header"))?;
    Ok(bytes)
}

/// The game-over sequence's assets.
pub struct GameOverAssets {
    /// `FLI/GO2.FLI`, the game-over animation.
    pub fli: Vec<u8>,
}

/// Load the game-over sequence's assets from the disc image.
pub fn load_gameover_assets(disc: &DiscImage) -> Result<GameOverAssets> {
    Ok(GameOverAssets {
        fli: load_fli_bytes(disc, "FLI/GO2.FLI")?,
    })
}

/// Load and decode the high-score screen's assets from the disc image.
pub fn load_highscore_assets(disc: &DiscImage) -> Result<HighscoreAssets> {
    let fli = load_fli_bytes(disc, "FLI/HIGHSCOR.FLI")?;

    let font_bytes = disc.read("FONT2.RAW").context("reading FONT2.RAW")?;
    let font = Font::decode(&font_bytes).context("decoding FONT2.RAW")?;

    Ok(HighscoreAssets { fli, font })
}

/// Synthetic, all-zero menu assets for tests that exercise scene logic without
/// the disc. Visually blank, but the right shapes.
#[cfg(test)]
pub(crate) fn test_menu_assets() -> MenuAssets {
    let background = IndexedImage::new(
        Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
        vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT) as usize],
    )
    .expect("synthetic background matches its dimensions");
    let font_sheet = vec![0u8; 320 * 62];
    let font = Font::decode(&font_sheet).expect("synthetic font sheet decodes");
    let palette = Palette::from_vga_6bit(&[0u8; 768]).expect("synthetic palette decodes");

    MenuAssets {
        background,
        font,
        palette,
    }
}

/// Synthetic intro assets for tests that exercise the intro's beat logic
/// without the disc. The stills are blank and the FLIs are empty: tests drive
/// the early stills/fades or skip the whole intro, and never reach a FLI beat.
#[cfg(test)]
pub(crate) fn test_intro_assets() -> IntroAssets {
    let still = || {
        let image = IndexedImage::new(
            Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
            vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT) as usize],
        )
        .expect("synthetic still matches its dimensions");
        let palette = Palette::from_vga_6bit(&[0u8; 768]).expect("synthetic palette decodes");

        StillImage { image, palette }
    };

    let font_sheet = vec![0u8; 320 * 62];
    let font = Font::decode(&font_sheet).expect("synthetic font sheet decodes");

    // The cover is taller than the screen like the real one (320x400), so the
    // intro's framebuffer swap is exercised without the disc.
    let cover = StillImage {
        image: blank_image(Dimensions::new(SCREEN_WIDTH, COVER_VISIBLE_HEIGHT)),
        palette: Palette::from_vga_6bit(&[0u8; 768]).expect("synthetic palette decodes"),
    };

    IntroAssets {
        neo: still(),
        surplogo: still(),
        cover,
        intro_fli: Vec::new(),
        fly_fli: Vec::new(),
        credz_fli: Vec::new(),
        font,
    }
}

/// A blank (all index 0) image of the given size, for synthetic test assets.
#[cfg(test)]
fn blank_image(size: Dimensions) -> IndexedImage {
    IndexedImage::new(size, vec![0u8; size.pixel_count()]).expect("blank image matches its size")
}

/// Synthetic, all-zero HUD assets for tests that exercise HUD/scene logic
/// without the disc. Blank, but the right shapes.
#[cfg(test)]
pub(crate) fn test_hud_assets() -> HudAssets {
    HudAssets {
        palette: Palette::from_vga_6bit(&[0u8; 768]).expect("synthetic palette decodes"),
        panel: blank_image(PANEL_SIZE),
        score_digits: blank_image(SCORE_DIGITS_SIZE),
        number_digits: blank_image(NUMBER_DIGITS_SIZE),
        weapon_bars: blank_image(WEAPON_BARS_SIZE),
        smart_frames: blank_image(SMART_FRAMES_SIZE),
        selector_lights: blank_image(SELECTOR_LIGHTS_SIZE),
        weapon_pods: blank_image(WEAPON_PODS_SIZE),
    }
}

/// A 1x1 transparent sprite for synthetic test assets.
#[cfg(test)]
fn blank_overlay() -> OverlaySprite {
    OverlaySprite {
        size: Dimensions::new(1, 1),
        pixels: vec![None],
    }
}

/// Synthetic level assets (blank canyon, blank HUD) for headless scene tests.
#[cfg(test)]
pub(crate) fn test_level_assets() -> LevelAssets {
    LevelAssets {
        background: Background::new(blank_image(BACKGROUND_SIZE), Sp::Canyon),
        hud: test_hud_assets(),
        overlays: PerWeapon::default().map(|()| OverlaySprite {
            size: Dimensions::new(1, 1),
            pixels: vec![None],
        }),
        overlay_slide: PerWeapon::splat([(0, 0); OVERLAY_FRAMES]),
        catalog: SpriteSheet {
            sprites: Vec::new(),
        },
        scenery: Scenery::new(Vec::new(), 0),
        ship_frames: SpriteSheet {
            sprites: Vec::new(),
        },
        ship: ShipData {
            catalog: 0,
            idle_frame: 0,
            flicker_frame: 27,
            y_min: -2,
            y_max: 110,
            explosion: None,
        },
        shield_frames: Vec::new(),
        fire_sprites: FireSprites {
            chaingun: blank_overlay(),
            multishot: [
                blank_overlay(),
                blank_overlay(),
                blank_overlay(),
                blank_overlay(),
            ],
            burning: [
                blank_overlay(),
                blank_overlay(),
                blank_overlay(),
                blank_overlay(),
            ],
            plasma_bolt: blank_overlay(),
            missile: std::array::from_fn(|_| blank_overlay()),
            bomb_ring: blank_overlay(),
            muzzle_flash: Vec::new(),
            plasma_orbs: std::array::from_fn(|_| std::array::from_fn(|_| blank_overlay())),
        },
        barrel_offsets: vec![(0, 0); ROLL_FRAMES],
        bob_wave: vec![0; 20],
        bomb_wave: vec![(0, 0); 32],
        sfx: SfxBank {
            samples: (0..16).map(|_| Arc::from(Vec::new())).collect(),
        },
        music: LevelMusic {
            track: 3,
            length_ticks: 10,
        },
        stars: &[],
        camera_min: 0,
        wad: Vec::new(),
        clip_catalog: SpriteSheet {
            sprites: Vec::new(),
        },
        cs_base: 0,
        spawn_rows: None,
        spawns: SpawnSource::StaticTable { table: 0 },
        spawn_ai: None,
        combat: Level::L1.data().combat,
        font: Font::decode(&[0u8; 320 * 16]).expect("synthetic font sheet decodes"),
        dim_table: [0; 256],
        ship_explosion: Vec::new(),
    }
}

/// Synthetic high-score assets (empty FLI, blank font) for headless tests.
#[cfg(test)]
pub(crate) fn test_highscore_assets() -> HighscoreAssets {
    let font_sheet = vec![0u8; 320 * 62];
    let font = Font::decode(&font_sheet).expect("synthetic font sheet decodes");

    HighscoreAssets {
        fli: Vec::new(),
        font,
    }
}

/// Synthetic game-over assets (empty FLI) for headless tests.
#[cfg(test)]
pub(crate) fn test_gameover_assets() -> GameOverAssets {
    GameOverAssets { fli: Vec::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// L1's segment-to-file base, used to lay out the test fixtures cs-relative.
    const TEST_CS_BASE: usize = 0x29F0;

    #[test]
    fn scenery_tilemap_maps_codes_skips_zero_and_follows_jumps() {
        // The decoder reads cs-relative (file = cs + cs_base). Two short streams
        // that jump to each other so the strip loops: stream A at cs 0x10 is
        // [code 3, code 1, empty, jump->0x20]; stream B at cs 0x20 is
        // [code 2, jump->0x10]. Codes map to cells n-1, 0 is an empty column.
        let mut wad = vec![0u8; 0x2a20];
        let put = |wad: &mut [u8], cs: usize, bytes: &[u8]| {
            let at = cs + TEST_CS_BASE;
            wad[at..at + bytes.len()].copy_from_slice(bytes);
        };
        put(&mut wad, 0x10, &[0x03, 0x01, 0x00, 0xff, 0x20, 0x00]);
        put(&mut wad, 0x20, &[0x02, 0xff, 0x10, 0x00]);

        let tiles = decode_scenery_tilemap(&wad, TEST_CS_BASE, -1, 0x10);

        // Exactly one loop: [cell 2, cell 0, gap, cell 1], stopping where the jump
        // returns to the start rather than wrapping at an arbitrary cut.
        assert_eq!(tiles, [Some(2), Some(0), None, Some(1)]);
    }

    #[test]
    fn scenery_tilemap_stops_at_the_end_of_the_wad() {
        // A stream with no jump runs out when the WAD does, not past it.
        let mut wad = vec![0u8; 0x29f0 + 3];
        wad[0x29f0] = 0x05; // cs 0, then the WAD ends mid-record

        let tiles = decode_scenery_tilemap(&wad, TEST_CS_BASE, -1, 0);

        assert_eq!(tiles, [Some(4)]);
    }
}

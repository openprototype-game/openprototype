//! The `.psg` savegame codec.
//!
//! The port's save format IS the original's: `protosgN.psg` is a full
//! mid-level snapshot the level writes from its in-game menu (the writer at
//! LEVEL_2 file `0xb8da`, the loader at `0xb9d3`; see `re/savegame.md`).
//! A file is the level's variable block (whose first byte is the level
//! number) followed by both parities of each live-object buffer -- A player
//! shots, B enemy shots, C entities, D fire staging, E effects, in that
//! order in every WAD. Everything is little-endian, no header, no padding.
//!
//! The block is one congruent structure in all seven WADs, relinked at
//! per-WAD addresses: the spawn table runs from its base to the cursor
//! word (whose baked initial value IS the base), then the scroll list and
//! its accumulators, then the ship cluster, then the score cluster, with
//! each level's own AI state woven around them. The races share every
//! delta but not their anchors (each race WAD's spawn table has its own
//! length). `BlockMap` carries the per-level anchors; everything else
//! sits at fixed deltas from them, validated field-by-field against real
//! DOSBox saves of all seven levels (`tests/fixtures/`).
//!
//! This module maps the snapshot onto the port's semantic state. Fields the
//! port does not model are written as the original's idle values and ignored
//! on read, annotated at their offset (open ones as `TODO`, decided
//! deviations in place). The freeze/dying flags and the engine PRNG live
//! outside the saved ranges, so a loaded game always resumes through GET
//! READY on a fresh clock seed, like the original. The per-level AI tails
//! (boss phase globals) are not carried yet: live entities restore with
//! their mode/phase words, but a save made mid-boss-fight resumes with the
//! boss's out-of-entity state reset.

use anyhow::{Result, bail};

use crate::level::l1::ai::BossState as L1Boss;
use crate::level::l3::ai::BossState as L3Boss;
use crate::level::l5::ai::BossState as L5Boss;
use crate::level::l7::ai::BossState as L7Boss;
use crate::level::slot::Record;
use crate::levels::Level;
use crate::spawns::{Effect, Entity, Shot};
use openprototype_core::game_state::Handoff;
use openprototype_core::{GameState, Lives, PerWeapon, SmartBombs, Weapon, WeaponLevel};

/// The variable block's cs base.
///
/// Byte 0 of the file is `cs:0xcb4`, the level number.
const BLOCK_BASE: usize = 0xCB4;

/// One shot buffer (A/B/D): a count word and 16-byte records.
const SHOT_BUF_LEN: usize = 0x640;
const SHOT_RECORD_LEN: usize = 16;
/// One entity record (C buffers; the per-level buffer length varies).
const ENTITY_RECORD_LEN: usize = 0x40;
/// One effects buffer (E): a count word and 399 16-byte records.
const EFFECT_BUF_LEN: usize = 0x1900;

/// The per-tick scroll constants each level's block carries.
///
/// Beside its accumulators. Ground truth from the fixtures; the tail entries
/// are the SP background strip rates, matching `background.rs`.
const RACE_SCROLL_CONSTS: &[u32] = &[0x30, 6, 0xA, 0x20];
const L1_SCROLL_CONSTS: &[u32] = &[0x10, 6, 0xA, 0x10, 0xA, 6, 3, 6, 0xA, 0x10];
const L3_SCROLL_CONSTS: &[u32] = &[0xA, 0x10, 0x20, 4, 0xA];
const L5_SCROLL_CONSTS: &[u32] = &[8, 0x10, 0xA, 8, 4, 1, 0];
#[rustfmt::skip]
const L7_SCROLL_CONSTS: &[u32] = &[
    0x10, 0x10, 0xA,
    // The 65-strip lava gradient, top to mirror bottom.
    0x100, 0xF8, 0xF1, 0xEA, 0xE3, 0xDB, 0xD4, 0xCD, 0xC6, 0xBE, 0xB7, 0xB0,
    0xA9, 0xA2, 0x9A, 0x93, 0x8C, 0x85, 0x7D, 0x76, 0x6F, 0x68, 0x61, 0x59,
    0x52, 0x4B, 0x44, 0x3C, 0x35, 0x2E, 0x27, 0x20, 0x20, 0x20, 0x27, 0x2E,
    0x35, 0x3C, 0x44, 0x4B, 0x52, 0x59, 0x61, 0x68, 0x6F, 0x76, 0x7D, 0x85,
    0x8C, 0x93, 0x9A, 0xA2, 0xA9, 0xB0, 0xB7, 0xBE, 0xC6, 0xCD, 0xD4, 0xDB,
    0xE3, 0xEA, 0xF1, 0xF8, 0x100,
];

/// One level's block geometry: the anchors that differ per WAD.
///
/// All other field addresses derive from these at fixed deltas (see the
/// accessors).
struct BlockMap {
    level_byte: u8,
    /// The block's length; the buffers follow it in the file.
    block_len: usize,
    /// The C (entity) buffer length: `0x640` (24 records) for L1/L3/L7,
    /// `0xc80` (49) for L5 and the races.
    entity_buf_len: usize,
    /// First spawn record (the cursor word's baked initial value).
    table_base: usize,
    /// The cursor word's cs address; the table region ends here.
    cursor_at: usize,
    /// The scroll accumulators' per-tick constants, count included.
    scroll_consts: &'static [u32],
    /// The score dword's cs address (the digit scratch and caches precede
    /// it, the extra-life threshold follows).
    score_at: usize,
    /// The win flag's cs address (`0xcc3` everywhere but L7's `0xcdf`; L7
    /// carries extra prefix fields that shift its flag/parity cluster).
    win_at: usize,
    /// The buffer parity selectors' base (A/B/C at +0, E at +0xa).
    parity_at: usize,
    /// The races' contact-grace word between invincibility and the camera
    /// pair; the shooters have no contact grace.
    has_grace: bool,
    /// The pod/orb stage byte. `0xcde` in the unshifted WADs; L3 and L7
    /// insert extra prefix fields that shift their fire var block (L3
    /// flags 0xcfa..0xcfd, L7 +0x1c across the cluster).
    stage_at: usize,
    /// The fire reload byte (`incb cooldown; cmp reload; jb` pair), where
    /// its offset is pinned: `0xcc8` unshifted, L7 `0xce4` (+0x1c). L3's
    /// insertion point inside the cluster is unpinned, so `None` there.
    reload_at: Option<usize>,
    /// The boss/scroll gate byte (`cs:0x269c` on L1). `None` until the
    /// level's boss state is wired into the codec.
    gate_at: Option<usize>,
}

impl BlockMap {
    fn of(level: Level) -> BlockMap {
        match level {
            Level::L1 => BlockMap {
                level_byte: 1,
                block_len: 0x1A09,
                entity_buf_len: 0x640,
                table_base: 0xCEC,
                cursor_at: 0x25EC,
                scroll_consts: L1_SCROLL_CONSTS,
                score_at: 0x268F,
                win_at: 0xCC3,
                parity_at: 0xCC5,
                has_grace: false,
                stage_at: 0xCDE,
                reload_at: Some(0xCC8),
                gate_at: Some(0x269C),
            },
            // The races share every delta, but each WAD's spawn table has its
            // own length, shifting the cursor and everything after it (L4
            // writer file 0xb964, loader 0xba64; L6 0xbe64/0xbf64).
            Level::L2 | Level::L4 | Level::L6 => {
                let (level_byte, block_len, cursor_at, score_at) = match level {
                    Level::L2 => (2, 0x1BE1, 0x27FE, 0x2873),
                    Level::L4 => (4, 0x1C59, 0x2876, 0x28EB),
                    _ => (6, 0x2159, 0x2D76, 0x2DEB),
                };

                BlockMap {
                    level_byte,
                    block_len,
                    entity_buf_len: 0xC80,
                    table_base: 0xCE6,
                    cursor_at,
                    scroll_consts: RACE_SCROLL_CONSTS,
                    score_at,
                    win_at: 0xCC3,
                    parity_at: 0xCC5,
                    has_grace: true,
                    stage_at: 0xCDE,
                    reload_at: Some(0xCC8),
                    gate_at: None,
                }
            }
            Level::L3 => BlockMap {
                level_byte: 3,
                block_len: 0x2CAF,
                entity_buf_len: 0x640,
                table_base: 0xD06,
                cursor_at: 0x38C6,
                scroll_consts: L3_SCROLL_CONSTS,
                score_at: 0x3941,
                win_at: 0xCC3,
                parity_at: 0xCC5,
                has_grace: false,
                stage_at: 0xCFE,
                reload_at: None,
                gate_at: Some(0x394E),
            },
            Level::L5 => BlockMap {
                level_byte: 5,
                block_len: 0x19EA,
                entity_buf_len: 0xC80,
                table_base: 0xCF1,
                cursor_at: 0x25F1,
                scroll_consts: L5_SCROLL_CONSTS,
                score_at: 0x267C,
                win_at: 0xCC3,
                parity_at: 0xCC5,
                has_grace: false,
                stage_at: 0xCDE,
                reload_at: Some(0xCC8),
                gate_at: Some(0x2689),
            },
            Level::L7 => BlockMap {
                level_byte: 7,
                block_len: 0x2235,
                entity_buf_len: 0x640,
                table_base: 0xD02,
                cursor_at: 0x2C52,
                scroll_consts: L7_SCROLL_CONSTS,
                score_at: 0x2EC7,
                win_at: 0xCDF,
                parity_at: 0xCE1,
                has_grace: false,
                stage_at: 0xCFA,
                reload_at: Some(0xCE4),
                gate_at: Some(0x2ED4),
            },
        }
    }

    fn level_of_byte(byte: u8) -> Option<Level> {
        match byte {
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

    fn scroll_count(&self) -> usize {
        self.scroll_consts.len()
    }

    /// The scroll list pointer (its value is [`Self::accums_at`]) and the count word.
    ///
    /// Right after the cursor.
    fn list_at(&self) -> usize {
        self.cursor_at + 2
    }

    fn accums_at(&self) -> usize {
        self.cursor_at + 6
    }

    /// The ship cluster's base.
    ///
    /// The fly-in ramp, then x/y, the two trail rings, the roll offset, the orb
    /// countdown, the respawn flag, lives, and invincibility, all at fixed
    /// deltas.
    fn ramp_at(&self) -> usize {
        self.accums_at() + self.scroll_count() * 8
    }

    /// The camera pair (`camera * 0x50`, then the camera row).
    ///
    /// The races keep their contact-grace word in front of it.
    fn camera50_at(&self) -> usize {
        self.ramp_at() + 0x2A + usize::from(self.has_grace) * 2
    }

    fn file_len(&self) -> usize {
        self.block_len
            + 4 * SHOT_BUF_LEN
            + 2 * self.entity_buf_len
            + 2 * SHOT_BUF_LEN
            + 2 * EFFECT_BUF_LEN
    }

    /// The byte offset of buffer copy `index`, after the block.
    ///
    /// Array order A/A B/B C/C D/D E/E (the writers dump them in this order in
    /// every WAD).
    fn buffer_at(&self, index: usize) -> usize {
        let sizes = [
            SHOT_BUF_LEN,
            SHOT_BUF_LEN,
            SHOT_BUF_LEN,
            SHOT_BUF_LEN,
            self.entity_buf_len,
            self.entity_buf_len,
            SHOT_BUF_LEN,
            SHOT_BUF_LEN,
            EFFECT_BUF_LEN,
            EFFECT_BUF_LEN,
        ];

        self.block_len + sizes[..index].iter().sum::<usize>()
    }
}

/// A decoded savegame: the semantic state the port restores a level from.
///
/// In-flight player shots (the A buffer) are deliberately dropped. The
/// original preserves them (the respawn/GET READY path at file `0x9d84`
/// does not clear the buffer), but the port models a player shot as a
/// `ShotKind` plus an octant, not the record's raw sprite pointer, and the
/// pointer is resolved per-level from the WAD; reconstructing it both ways
/// would couple the WAD into this codec for shots that live well under a
/// second and self-correct on the first unfrozen tick. The fire system's
/// transient state (cooldown, muzzle flash) resumes idle for the same
/// reason GET READY makes it unobservable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveGame {
    pub level: Level,
    /// Score, lives, bombs, weapon levels, selection, invincibility, grace.
    pub state: GameState,
    /// The ISR-mutated spawn schedule (the head record's delay decays in
    /// place) and the consumer's position in it.
    pub records: Vec<Record>,
    pub cursor: usize,
    /// The weapon-upgrade-drop countdown.
    pub weapon_upgrade_drop_countdown: i32,
    /// The win flag.
    pub level_end: bool,
    pub entities: Vec<Entity>,
    pub enemy_shots: Vec<Shot>,
    pub effects: Vec<Effect>,
    /// Ship x/y in pixels, the fly-in ramp counter, the roll frame.
    pub ship_x: i32,
    pub ship_y: i32,
    pub ship_ramp: i32,
    pub ship_roll: i32,
    /// The scroll accumulators, as many as the level keeps in its saved
    /// list. The SP background strips start at index 3 in every level; the
    /// leading entries drive the scenery layers (and the races' two star
    /// planes).
    pub scroll_accums: Vec<u32>,
    /// The camera row (the races store their speed here; it is their
    /// camera).
    pub speed_level: u16,
    /// The boss/scroll gate (`cs:0x269c` on L1; per-level offset elsewhere).
    pub gate: u8,
    /// The boss engine globals that live outside the entity records.
    pub(crate) boss: BossSave,
}

/// The per-level boss engine globals carried through a save.
///
/// Each level's boss keeps its phase state in `cs:[...]` globals inside the
/// saved block rather than in the entity record, so the codec carries them
/// separately. The shooter levels are wired; races round-trip as `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BossSave {
    /// No boss globals carried (races).
    None,
    /// The L1 boss globals.
    L1(L1Boss),
    /// The L3 boss globals.
    L3(L3Boss),
    /// The L5 boss globals.
    L5(L5Boss),
    /// The L7 composite-boss globals.
    L7(L7Boss),
}

impl SaveGame {
    /// The carried payload embedded in the snapshot.
    ///
    /// For code that wants the handoff view of it.
    pub fn handoff(&self) -> Handoff {
        self.state.handoff()
    }

    /// Encodes as the original `.psg` for this save's level.
    pub fn encode(&self) -> Vec<u8> {
        let map = BlockMap::of(self.level);
        let mut bytes = vec![0u8; map.file_len()];
        self.encode_block(&map, &mut bytes[..map.block_len]);

        // A: player shots, both parities. Left empty by design -- see the
        // SaveGame doc; the in-flight shots self-correct within a frame.

        // B: enemy shots, both parities (identical copies are consistent;
        // the engine reads the parity the block names, which we write as 0).
        for parity in 0..2 {
            let at = map.buffer_at(2 + parity);
            encode_shots(&mut bytes[at..at + SHOT_BUF_LEN], &self.enemy_shots);
        }

        // C: entities, both parities.
        for parity in 0..2 {
            let at = map.buffer_at(4 + parity);
            encode_entities(
                &mut bytes[at..at + map.entity_buf_len],
                &self.entities,
                map.entity_buf_len,
            );
        }

        // D: the fire-staging buffer, merged into A every frame; transient,
        // saved empty.

        // E: effects, both parities.
        for parity in 0..2 {
            let at = map.buffer_at(8 + parity);
            encode_effects(&mut bytes[at..at + EFFECT_BUF_LEN], &self.effects);
        }

        bytes
    }

    /// Decodes a `.psg` of any level.
    pub fn decode(bytes: &[u8]) -> Result<SaveGame> {
        let Some(level) = bytes.first().copied().and_then(BlockMap::level_of_byte) else {
            bail!("not a savegame: unknown level byte");
        };

        let map = BlockMap::of(level);

        if bytes.len() != map.file_len() {
            bail!(
                "savegame is {} bytes, level {} files are {}",
                bytes.len(),
                map.level_byte,
                map.file_len()
            );
        }

        let block = &bytes[..map.block_len];
        let word =
            |at: usize| u16::from_le_bytes([block[at - BLOCK_BASE], block[at - BLOCK_BASE + 1]]);
        let dword = |at: usize| {
            let at = at - BLOCK_BASE;
            u32::from_le_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]])
        };
        let byte = |at: usize| block[at - BLOCK_BASE];

        // The parity words select which copy of each pair is current.
        let parity_abc = usize::from(word(map.parity_at)) / 2;
        let parity_e = usize::from(word(map.parity_at + 0xA)) / 2;

        let selected = match word(0xCB7) {
            2 => Weapon::Burning,
            3 => Weapon::Plasma,
            4 => Weapon::Missile,
            _ => Weapon::Multishot,
        };
        let mut weapons = PerWeapon::splat(WeaponLevel::new(0));

        for (index, weapon) in Weapon::ALL.iter().enumerate() {
            let fill = word(0xCB9 + index * 2);
            *weapons.get_mut(*weapon) = WeaponLevel::new((fill / 8) as u8);
        }

        let ramp = map.ramp_at();
        let state = GameState {
            score: dword(map.score_at),
            lives: Lives::new(byte(ramp + 0x27)),
            smart_bombs: SmartBombs::new(word(0xCC1) as u8),
            weapons,
            selected,
            invincible_ticks: word(ramp + 0x28),
            // The shooters have no contact grace (a race mechanic).
            contact_grace_ticks: if map.has_grace { word(ramp + 0x2A) } else { 0 },
        };

        let table = &block[map.table_base - BLOCK_BASE..map.cursor_at - BLOCK_BASE];
        let mut records = Vec::new();

        for raw in table.chunks_exact(8) {
            let field = |k: usize| u16::from_le_bytes([raw[k], raw[k + 1]]);

            // The table region holds the whole scratch area; records end at
            // the first zero-sprite terminator, like the level-init parse.
            if field(2) == 0 {
                break;
            }

            records.push(Record {
                delay: field(0),
                sprite: field(2),
                health: field(4),
                spawn_row: field(6),
            });
        }

        let cursor_pointer = usize::from(word(map.cursor_at));
        let cursor = cursor_pointer.saturating_sub(map.table_base) / 8;

        let scroll_accums = (0..map.scroll_count())
            .map(|index| dword(map.accums_at() + index * 4))
            .collect();

        let buffer = |index: usize, len: usize| {
            let at = map.buffer_at(index);

            &bytes[at..at + len]
        };

        let enemy_shots = decode_shots(buffer(2 + parity_abc, SHOT_BUF_LEN));
        let entities = decode_entities(buffer(4 + parity_abc, map.entity_buf_len));
        let effects = decode_effects(buffer(8 + parity_e, EFFECT_BUF_LEN));

        Ok(SaveGame {
            level,
            state,
            records,
            cursor,
            weapon_upgrade_drop_countdown: i32::from(word(ramp + 0x24) as i16),
            level_end: byte(map.win_at) == 1,
            entities,
            enemy_shots,
            effects,
            ship_x: i32::from(word(ramp + 2) as i16),
            ship_y: i32::from(word(ramp + 4) as i16),
            ship_ramp: i32::from(word(ramp) as i16),
            ship_roll: i32::from(word(ramp + 0x22) as i16) / 0x12,
            scroll_accums,
            speed_level: word(map.camera50_at() + 2),
            gate: map.gate_at.map_or(0, byte),
            boss: match level {
                Level::L1 => BossSave::L1(L1Boss::restore_from(bytes, BLOCK_BASE)),
                Level::L3 => BossSave::L3(L3Boss::restore_from(bytes, BLOCK_BASE)),
                Level::L5 => BossSave::L5(L5Boss::restore_from(bytes, BLOCK_BASE)),
                Level::L7 => BossSave::L7(L7Boss::restore_from(bytes, BLOCK_BASE)),
                _ => BossSave::None,
            },
        })
    }

    /// Fills the variable block.
    ///
    /// The mapped fields, the entity records, the scroll gate, and the boss
    /// engine globals (where wired). Remaining unmodeled scratch (other
    /// levels' AI tails, prefix extras) stays zero.
    fn encode_block(&self, map: &BlockMap, block: &mut [u8]) {
        fn put_word(block: &mut [u8], at: usize, value: u16) {
            block[at - BLOCK_BASE..at - BLOCK_BASE + 2].copy_from_slice(&value.to_le_bytes());
        }

        // Weapons: a load resumes through GET READY (fire-released-then-
        // pressed), so the firing weapon re-resolves on the first unfrozen
        // tick in both engines; the stored cs:0xcb5 is never a held-burst
        // continuation. Storing the selected weapon (0 on empty) matches.
        let firing = match self.state.active_weapon() {
            openprototype_core::ActiveWeapon::Chaingun => 0,
            openprototype_core::ActiveWeapon::Selected(weapon) => weapon_index(weapon),
        };
        put_word(block, 0xCB5, firing);
        put_word(block, 0xCB7, weapon_index(self.state.selected));

        for (index, weapon) in Weapon::ALL.iter().enumerate() {
            let fill = u16::from(self.state.level(*weapon).get()) * 8;
            put_word(block, 0xCB9 + index * 2, fill);
        }

        put_word(block, 0xCC1, u16::from(self.state.smart_bombs.get()));
        block[map.win_at - BLOCK_BASE] = u8::from(self.level_end);
        // Parities: both copies are written identically, current = 0.

        // The fire/pod engine prefix restores to its baked image state
        // rather than zeros. Reload = 6 (the consumer is incb cooldown/
        // cmp reload/jb at L2 0x9211, so a written 0 means every-tick fire
        // until the next weapon resolve); the pod/orb stage byte = 1 (the
        // hold stage -- all 14 consumers in L2 0x8b76..0x8c50 compare
        // against 1..4 and nothing else ever writes it, so a 0 kills the
        // orb machine for the rest of an original-engine session).
        // Cooldown, flash and the orb flags stay 0 like a fresh level. The
        // baked reload/stage above already avoids the original's zeroing bug
        // (a written 0 stage killed the orb machine for the rest of a
        // cross-engine session); the live values are not carried by design --
        // a load resumes through GET READY, which makes this transient fire
        // state unobservable after resume.
        if let Some(reload_at) = map.reload_at {
            block[reload_at - BLOCK_BASE] = 6;
        }

        block[map.stage_at - BLOCK_BASE] = 1;

        if map.has_grace {
            // The races' HUD alternation flag (L2 cs:0x2862, score - 0x11
            // in every race WAD; baked 1): the original notb-toggles it,
            // so a written 0 sticks the alternation off-phase forever.
            // The bookkeeping words before it are baked 0.
            block[map.score_at - 0x11 - BLOCK_BASE] = 1;
        }

        for (index, record) in self.records.iter().enumerate() {
            let at = map.table_base - BLOCK_BASE + index * 8;
            block[at..at + 2].copy_from_slice(&record.delay.to_le_bytes());
            block[at + 2..at + 4].copy_from_slice(&record.sprite.to_le_bytes());
            block[at + 4..at + 6].copy_from_slice(&record.health.to_le_bytes());
            block[at + 6..at + 8].copy_from_slice(&record.spawn_row.to_le_bytes());
        }

        put_word(
            block,
            map.cursor_at,
            (map.table_base + self.cursor * 8) as u16,
        );

        // Scroll bookkeeping: the static list pointer/count, then the live
        // accumulators and their per-tick constants (dwords).
        put_word(block, map.list_at(), map.accums_at() as u16);
        put_word(block, map.list_at() + 2, map.scroll_count() as u16);

        for index in 0..map.scroll_count() {
            let accum = self.scroll_accums.get(index).copied().unwrap_or(0);
            let at = map.accums_at() - BLOCK_BASE + index * 4;
            block[at..at + 4].copy_from_slice(&accum.to_le_bytes());

            let at = at + map.scroll_count() * 4;
            block[at..at + 4].copy_from_slice(&map.scroll_consts[index].to_le_bytes());
        }

        let ramp = map.ramp_at();

        // The ship-trail history rings (x then y, 7 words each); a save
        // parked on the current position is consistent.
        for slot in 0..7 {
            put_word(block, ramp + 6 + slot * 2, self.ship_x as u16);
            put_word(block, ramp + 0x14 + slot * 2, self.ship_y as u16);
        }

        put_word(block, ramp, self.ship_ramp as u16);
        put_word(block, ramp + 2, self.ship_x as u16);
        put_word(block, ramp + 4, self.ship_y as u16);
        put_word(block, ramp + 0x22, (self.ship_roll * 0x12) as u16);
        put_word(
            block,
            ramp + 0x24,
            self.weapon_upgrade_drop_countdown as u16,
        );
        block[ramp + 0x27 - BLOCK_BASE] = self.state.lives.get();
        put_word(block, ramp + 0x28, self.state.invincible_ticks);

        if map.has_grace {
            put_word(block, ramp + 0x2A, self.state.contact_grace_ticks);
        }

        put_word(block, map.camera50_at(), self.speed_level * 0x50);
        put_word(block, map.camera50_at() + 2, self.speed_level);

        // The score cluster: the six-digit readout scratch, the caches as
        // the in-level load path reseeds them (0x0b = blank, forcing a full
        // redraw), the score itself, and the extra-life decade threshold.
        for place in 0..6 {
            let digit = (self.state.score / 10u32.pow(5 - place as u32) % 10) as u8;
            block[map.score_at - 12 + place - BLOCK_BASE] = digit;
        }

        block[map.score_at - 6 - BLOCK_BASE..map.score_at - BLOCK_BASE].fill(0x0B);
        block[map.score_at - BLOCK_BASE..map.score_at - BLOCK_BASE + 4]
            .copy_from_slice(&self.state.score.to_le_bytes());
        put_word(block, map.score_at + 4, (self.state.score / 10_000) as u16);

        // The in-level-load housekeeping byte and the HUD redraw flag as a
        // running game carries them.
        block[map.score_at + 6 - BLOCK_BASE] = 1;
        block[map.score_at + 9 - BLOCK_BASE] = 1;

        block[0] = map.level_byte;

        // The boss/scroll gate and the boss's out-of-entity phase globals,
        // so a save taken mid-boss resumes the boss in place rather than
        // restarting its pattern.
        if let Some(gate_at) = map.gate_at {
            block[gate_at - BLOCK_BASE] = self.gate;
        }

        match &self.boss {
            BossSave::L1(boss) => boss.save_into(block, BLOCK_BASE),
            BossSave::L3(boss) => boss.save_into(block, BLOCK_BASE),
            BossSave::L5(boss) => boss.save_into(block, BLOCK_BASE),
            BossSave::L7(boss) => boss.save_into(block, BLOCK_BASE),
            BossSave::None => {}
        }
    }
}

/// What one saved scroll-accumulator slot drives.
///
/// For the save/restore mapping between the list and the port's scroll state.
#[derive(Clone, Copy)]
pub enum ScrollSlot {
    /// A scenery layer's accumulator (the port's layer index; the save
    /// order does not always match the layer order -- L1's three layers
    /// save as {16, 6, 10} against draw order {6, 10, 16}).
    Scenery(usize),
    /// A free-running accumulator the port does not model: the races' two
    /// star planes (clock-scattered on every init, not restorable in the
    /// original either) and the dead layer slots some levels keep ticking.
    /// Saved as its rate times the elapsed ticks, which matches the
    /// original until the reference strip's first wrap.
    Derived,
}

/// One level's saved accumulator order.
///
/// The leading slots, then the SP background strips one-to-one, then any
/// trailing slots (L3 keeps a dead slot after its strip).
pub struct ScrollLayout {
    pub leading: &'static [ScrollSlot],
    pub strips: usize,
    pub trailing: &'static [ScrollSlot],
}

/// The level's saved accumulator order.
///
/// Ground truth: the fixtures' rate constants matched against the port's layer
/// and strip tables.
pub fn scroll_layout(level: Level) -> ScrollLayout {
    use ScrollSlot::{Derived, Scenery};

    match level {
        Level::L1 => ScrollLayout {
            leading: &[Scenery(2), Scenery(0), Scenery(1)],
            strips: 7,
            trailing: &[],
        },
        Level::L2 | Level::L4 | Level::L6 => ScrollLayout {
            leading: &[Scenery(0), Derived, Derived],
            strips: 1,
            trailing: &[],
        },
        Level::L3 => ScrollLayout {
            leading: &[Scenery(0), Scenery(1), Scenery(2)],
            strips: 1,
            trailing: &[Derived],
        },
        Level::L5 => ScrollLayout {
            leading: &[Scenery(0), Scenery(1), Derived],
            strips: 4,
            trailing: &[],
        },
        Level::L7 => ScrollLayout {
            leading: &[Scenery(0), Scenery(1), Derived],
            strips: 65,
            trailing: &[],
        },
    }
}

/// The per-tick rate of each saved accumulator slot.
///
/// For the derived slots and the elapsed-tick estimate (the first strip's rate
/// is at index `leading.len()`).
pub fn scroll_consts(level: Level) -> &'static [u32] {
    BlockMap::of(level).scroll_consts
}

fn weapon_index(weapon: Weapon) -> u16 {
    match weapon {
        Weapon::Multishot => 1,
        Weapon::Burning => 2,
        Weapon::Plasma => 3,
        Weapon::Missile => 4,
    }
}

/// Writes a shot list as a count word plus 16-byte records.
fn encode_shots(buffer: &mut [u8], shots: &[Shot]) {
    let count = shots.len().min((SHOT_BUF_LEN - 2) / SHOT_RECORD_LEN);
    buffer[0..2].copy_from_slice(&(count as u16).to_le_bytes());

    for (index, shot) in shots.iter().take(count).enumerate() {
        let at = 2 + index * SHOT_RECORD_LEN;
        buffer[at..at + 2].copy_from_slice(&shot.sprite.to_le_bytes());
        buffer[at + 2..at + 4].copy_from_slice(&(shot.x as u16).to_le_bytes());
        buffer[at + 4..at + 6].copy_from_slice(&(shot.y as u16).to_le_bytes());
        buffer[at + 6..at + 8].copy_from_slice(&(shot.vx as u16).to_le_bytes());
        buffer[at + 8..at + 10].copy_from_slice(&(shot.vy as u16).to_le_bytes());
        // +0xa/+0xb (size) and +0xe (damage) stay zero. The spawn helper
        // (file 0xdd78) copies size from the sprite descriptor +8/+9 -- the
        // same bytes the port re-derives at collision time -- and never
        // writes +0xe (the enemy hit consequence 0xc4b7 ignores it).
        // Reconstructing size would need the WAD in this WAD-free codec, so
        // it is left zero; cross-engine, an in-flight enemy shot loses its
        // collision box in the original until it expires.
    }
}

fn decode_shots(buffer: &[u8]) -> Vec<Shot> {
    let count = usize::from(u16::from_le_bytes([buffer[0], buffer[1]]))
        .min((SHOT_BUF_LEN - 2) / SHOT_RECORD_LEN);

    (0..count)
        .map(|index| {
            let at = 2 + index * SHOT_RECORD_LEN;
            let word =
                |k: usize| i32::from(i16::from_le_bytes([buffer[at + k], buffer[at + k + 1]]));

            Shot {
                sprite: u16::from_le_bytes([buffer[at], buffer[at + 1]]),
                x: word(2),
                y: word(4),
                vx: word(6),
                vy: word(8),
            }
        })
        .collect()
}

/// Writes the entity list as a count word plus 0x40-byte records.
///
/// The entity layout is from `re/spawn-consumer.md`.
fn encode_entities(buffer: &mut [u8], entities: &[Entity], buf_len: usize) {
    let count = entities.len().min((buf_len - 2) / ENTITY_RECORD_LEN);
    buffer[0..2].copy_from_slice(&(count as u16).to_le_bytes());

    for (index, entity) in entities.iter().take(count).enumerate() {
        let at = 2 + index * ENTITY_RECORD_LEN;
        let mut put_word = |k: usize, value: u16| {
            buffer[at + k..at + k + 2].copy_from_slice(&value.to_le_bytes());
        };

        put_word(0, entity.sprite);
        put_word(2, entity.kind);
        put_word(4, entity.x as u16);
        put_word(6, entity.y as u16);
        put_word(0x14, entity.mode);
        put_word(0x16, entity.arg);
        put_word(0x18, entity.health as u16);
        // The fire-pattern word (+0x1b, 0xffff = none) stays unset by design:
        // the original's pattern pass (table cs:0x46b3) is latent in the
        // shipping game -- DOSBox-confirmed inert -- and the port drives enemy
        // fire from the AI functions directly. Intentional deviation.
        put_word(0x1B, 0xFFFF);
        put_word(0x20, entity.tick);
        put_word(0x22, entity.phase_a);
        put_word(0x24, entity.phase_b);
        put_word(0x26, entity.save_y as u16);
        put_word(0x28, entity.save_x as u16);
        put_word(0x2A, entity.counter);

        buffer[at + 0x1A] = u8::from(entity.seen);
        buffer[at + 0x1F] = entity.anim;

        for (box_index, hitbox) in entity.hitboxes.iter().enumerate() {
            buffer[at + 8 + box_index * 4..at + 8 + box_index * 4 + 4].copy_from_slice(hitbox);
        }
    }
}

fn decode_entities(buffer: &[u8]) -> Vec<Entity> {
    let count = usize::from(u16::from_le_bytes([buffer[0], buffer[1]]))
        .min((buffer.len() - 2) / ENTITY_RECORD_LEN);

    (0..count)
        .map(|index| {
            let at = 2 + index * ENTITY_RECORD_LEN;
            let word = |k: usize| u16::from_le_bytes([buffer[at + k], buffer[at + k + 1]]);
            let signed = |k: usize| i32::from(word(k) as i16);

            let mut hitboxes = [[0u8; 4]; 3];

            for (box_index, hitbox) in hitboxes.iter_mut().enumerate() {
                hitbox.copy_from_slice(&buffer[at + 8 + box_index * 4..at + 8 + box_index * 4 + 4]);
            }

            Entity {
                sprite: word(0),
                kind: word(2),
                x: signed(4),
                y: signed(6),
                mode: word(0x14),
                arg: word(0x16),
                health: signed(0x18),
                seen: buffer[at + 0x1A] != 0,
                anim: buffer[at + 0x1F],
                tick: word(0x20),
                // The death handler reads the debris template from the
                // type descriptor, not the record; the loader re-derives it.
                debris: 0,
                hitboxes,
                phase_a: word(0x22),
                phase_b: word(0x24),
                save_y: signed(0x26),
                save_x: signed(0x28),
                counter: word(0x2A),
            }
        })
        .collect()
}

/// Writes the effects list as a count word plus 16-byte records.
fn encode_effects(buffer: &mut [u8], effects: &[Effect]) {
    let count = effects.len().min((EFFECT_BUF_LEN - 2) / 16);
    buffer[0..2].copy_from_slice(&(count as u16).to_le_bytes());

    for (index, effect) in effects.iter().take(count).enumerate() {
        let at = 2 + index * 16;
        buffer[at..at + 2].copy_from_slice(&effect.sprite.to_le_bytes());
        buffer[at + 2..at + 4].copy_from_slice(&(effect.x as u16).to_le_bytes());
        buffer[at + 4..at + 6].copy_from_slice(&(effect.y as u16).to_le_bytes());
        buffer[at + 6] = effect.frames;
        buffer[at + 7] = effect.rate;
        buffer[at + 8..at + 10].copy_from_slice(&effect.step.to_le_bytes());
        buffer[at + 0xA] = effect.phase;
        buffer[at + 0xB..at + 0xD].copy_from_slice(&effect.delay.to_le_bytes());
    }
}

fn decode_effects(buffer: &[u8]) -> Vec<Effect> {
    let count =
        usize::from(u16::from_le_bytes([buffer[0], buffer[1]])).min((EFFECT_BUF_LEN - 2) / 16);

    (0..count)
        .map(|index| {
            let at = 2 + index * 16;
            let word = |k: usize| u16::from_le_bytes([buffer[at + k], buffer[at + k + 1]]);

            Effect {
                sprite: word(0),
                x: i32::from(word(2) as i16),
                y: i32::from(word(4) as i16),
                frames: buffer[at + 6],
                rate: buffer[at + 7],
                step: word(8),
                phase: buffer[at + 0xA],
                delay: word(0xB),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> SaveGame {
        SaveGame {
            level: Level::L2,
            state: GameState {
                score: 123_456,
                lives: Lives::new(2),
                smart_bombs: SmartBombs::new(3),
                weapons: {
                    let mut weapons = PerWeapon::splat(WeaponLevel::new(0));
                    weapons.burning = WeaponLevel::new(3);
                    weapons
                },
                selected: Weapon::Burning,
                invincible_ticks: 17,
                contact_grace_ticks: 5,
            },
            records: vec![
                Record {
                    delay: 7,
                    sprite: 0x3C3C,
                    health: 32000,
                    spawn_row: 12,
                },
                Record {
                    delay: 400,
                    sprite: 0x3E1C,
                    health: 0x14,
                    spawn_row: 209,
                },
            ],
            cursor: 1,
            weapon_upgrade_drop_countdown: 4,
            level_end: false,
            entities: vec![Entity {
                sprite: 0x3C5A,
                kind: 0x3C5A,
                x: 100 << 4,
                y: 60 << 4,
                mode: 0,
                arg: 2,
                health: 32000,
                seen: true,
                anim: 3,
                tick: 99,
                debris: 0,
                hitboxes: [
                    [0x0C, 0x04, 0x38, 0x19],
                    [0x1C, 0x02, 0x2F, 0x1E],
                    [2, 13, 0x40, 0x16],
                ],
                phase_a: 1,
                phase_b: 2,
                save_y: -3,
                save_x: 4,
                counter: 5,
            }],
            enemy_shots: vec![Shot {
                sprite: 0x3608,
                x: 50 << 4,
                y: 40 << 4,
                vx: 0x78,
                vy: -8,
            }],
            effects: vec![Effect {
                sprite: 0x3B48,
                x: 88,
                y: 44,
                frames: 10,
                rate: 4,
                step: 8,
                phase: 1,
                delay: 0,
            }],
            ship_x: 120,
            ship_y: 70,
            ship_ramp: 10,
            ship_roll: 13,
            scroll_accums: vec![0x1234, 0x5678, 0x9ABC, 0xDEF0],
            speed_level: 0x10,
            gate: 0,
            boss: BossSave::None,
        }
    }

    #[test]
    fn the_race_file_has_the_original_length() {
        assert_eq!(sample().encode().len(), 0x8C61);
    }

    #[test]
    fn the_level_byte_leads_the_file() {
        assert_eq!(sample().encode()[0], 2);
    }

    #[test]
    fn each_race_wad_has_its_own_geometry() {
        let mut save = sample();

        save.level = Level::L4;
        let encoded = save.encode();
        assert_eq!(encoded.len(), 36_057);
        assert_eq!(SaveGame::decode(&encoded).expect("decodes"), save);

        save.level = Level::L6;
        let encoded = save.encode();
        assert_eq!(encoded.len(), 37_337);
        assert_eq!(SaveGame::decode(&encoded).expect("decodes"), save);
    }

    #[test]
    fn a_round_trip_preserves_the_modeled_state() {
        let save = sample();
        let decoded = SaveGame::decode(&save.encode()).expect("decodes");

        assert_eq!(decoded, save);
    }

    #[test]
    fn a_shooter_round_trip_preserves_the_modeled_state() {
        let mut save = sample();
        save.level = Level::L1;
        save.state.contact_grace_ticks = 0; // shooters carry no grace
        save.scroll_accums = (0..10).map(|index| index * 0x111).collect();
        // An L1 file always decodes to L1 boss globals (idle defaults here).
        save.boss = BossSave::L1(L1Boss::default());

        let encoded = save.encode();
        assert_eq!(encoded.len(), 32_265);
        assert_eq!(SaveGame::decode(&encoded).expect("decodes"), save);
    }

    /// Locks the full L1 encode (boss tail included) against accidental drift.
    ///
    /// FNV-1a over the encoded bytes; deterministic across platforms, so the
    /// digest only changes when the encoded form does.
    #[test]
    fn the_l1_encode_matches_its_golden() {
        let mut save = sample();
        save.level = Level::L1;
        save.state.contact_grace_ticks = 0;
        save.scroll_accums = (0..10).map(|index| index * 0x111).collect();
        save.boss = BossSave::L1(L1Boss::default());

        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;

        for byte in save.encode() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }

        assert_eq!(format!("{hash:016x}"), "0f87e94181997ed8");
    }

    /// Real saves created by the original engine in DOSBox: one chained
    /// capture session (L2 entered with a 25,000-point carry, then the
    /// shooters in level order).
    const DOSBOX_RACE_SAVE: &[u8] = include_bytes!("../tests/fixtures/l2-race.psg");
    const DOSBOX_L1_SAVE: &[u8] = include_bytes!("../tests/fixtures/l1.psg");
    const DOSBOX_L3_SAVE: &[u8] = include_bytes!("../tests/fixtures/l3.psg");
    const DOSBOX_L5_SAVE: &[u8] = include_bytes!("../tests/fixtures/l5.psg");
    const DOSBOX_L7_SAVE: &[u8] = include_bytes!("../tests/fixtures/l7.psg");
    const DOSBOX_L4_SAVE: &[u8] = include_bytes!("../tests/fixtures/l4-race.psg");
    const DOSBOX_L6_SAVE: &[u8] = include_bytes!("../tests/fixtures/l6-race.psg");

    #[test]
    fn the_dosbox_race_save_decodes() {
        let save = SaveGame::decode(DOSBOX_RACE_SAVE).expect("ground truth decodes");

        assert_eq!(save.level, Level::L2);
        assert_eq!(save.state.score, 25_000);
        assert_eq!(save.state.lives.get(), 4);
        assert_eq!(save.state.smart_bombs.get(), 2);
        // The capture entered with a carry of {2, 1, 3, 1} weapon levels.
        assert_eq!(save.state.level(Weapon::Multishot).get(), 2);
        assert_eq!(save.state.level(Weapon::Burning).get(), 1);
        assert_eq!(save.state.level(Weapon::Plasma).get(), 3);
        assert_eq!(save.state.level(Weapon::Missile).get(), 1);
        assert_eq!(save.records.len(), 67);
        assert_eq!(save.cursor, 5);
        assert_eq!(save.weapon_upgrade_drop_countdown, 3);
        assert!(!save.level_end);
        assert_eq!((save.ship_x, save.ship_y), (120, 23));
        assert_eq!(save.ship_ramp, 10);
        assert_eq!(save.ship_roll, 8);
        assert_eq!(save.speed_level, 0);
        assert_eq!(save.scroll_accums, vec![0x30C0, 0x618, 0xA28, 0x2080]);
        // Five live obstacles, two carrying the player's symbolic shot
        // damage, including the table's stack-of-three thin bars.
        assert_eq!(save.entities.len(), 5);
        assert_eq!(save.entities[0].kind, 0x3E1C);
        assert_eq!(save.entities[0].health, 31_964);
        assert_eq!(save.entities[1].kind, 0x3F2A);
        assert_eq!(save.entities[2].kind, 0x3EB2);
        assert_eq!(save.entities[2].health, 31_928);
        assert_eq!(save.entities[4].kind, 0x3EB2);
        assert!(save.enemy_shots.is_empty());
        assert!(save.effects.is_empty());
    }

    #[test]
    fn the_dosbox_l1_save_decodes() {
        let save = SaveGame::decode(DOSBOX_L1_SAVE).expect("ground truth decodes");

        assert_eq!(save.level, Level::L1);
        assert_eq!(save.state.score, 25_170);
        assert_eq!(save.state.lives.get(), 5);
        assert_eq!(save.state.level(Weapon::Multishot).get(), 4);
        assert_eq!(save.state.level(Weapon::Burning).get(), 1);
        assert_eq!(save.state.selected, Weapon::Multishot);
        assert_eq!(save.cursor, 12);
        assert_eq!((save.ship_x, save.ship_y), (116, 73));
        assert_eq!(save.ship_ramp, 10);
        assert_eq!(save.ship_roll, 0);
        assert_eq!(save.speed_level, 22);
        assert_eq!(save.weapon_upgrade_drop_countdown, 7);
        assert_eq!(save.scroll_accums.len(), 10);
        assert_eq!(save.entities.len(), 1);
        assert_eq!(save.entities[0].kind, 0x3308);
        assert_eq!(save.entities[0].health, 40);
        assert_eq!(save.effects.len(), 3);
        assert!(save.enemy_shots.is_empty());
    }

    #[test]
    fn the_dosbox_l3_save_decodes() {
        let save = SaveGame::decode(DOSBOX_L3_SAVE).expect("ground truth decodes");

        assert_eq!(save.level, Level::L3);
        assert_eq!(save.state.score, 25_821);
        assert_eq!(save.state.lives.get(), 5);
        assert_eq!(save.cursor, 15);
        assert_eq!((save.ship_x, save.ship_y), (128, 121));
        // The side-view idle pose: roll frame 21.
        assert_eq!(save.ship_roll, 21);
        assert_eq!(save.speed_level, 32);
        assert_eq!(save.weapon_upgrade_drop_countdown, 5);
        assert_eq!(save.scroll_accums.len(), 5);
        assert_eq!(save.entities.len(), 1);
        assert_eq!(save.entities[0].kind, 0x58B0);
        assert_eq!(save.entities[0].health, 190);
        assert!(save.effects.is_empty());
    }

    #[test]
    fn the_dosbox_l5_save_decodes() {
        let save = SaveGame::decode(DOSBOX_L5_SAVE).expect("ground truth decodes");

        assert_eq!(save.level, Level::L5);
        assert_eq!(save.state.score, 25_961);
        assert_eq!(save.state.level(Weapon::Burning).get(), 2);
        assert_eq!(save.cursor, 13);
        assert_eq!((save.ship_x, save.ship_y), (100, 91));
        assert_eq!(save.ship_roll, 3);
        assert_eq!(save.speed_level, 32);
        assert_eq!(save.weapon_upgrade_drop_countdown, 6);
        assert_eq!(save.scroll_accums.len(), 7);
        assert_eq!(save.entities.len(), 4);
        assert_eq!(save.entities[0].kind, 0x3C4E);
        assert_eq!(save.entities[0].health, 50);
        assert_eq!(save.effects.len(), 2);
    }

    #[test]
    fn the_dosbox_l7_save_decodes() {
        let save = SaveGame::decode(DOSBOX_L7_SAVE).expect("ground truth decodes");

        assert_eq!(save.level, Level::L7);
        assert_eq!(save.state.score, 26_060);
        assert_eq!(save.state.smart_bombs.get(), 2);
        assert_eq!(save.state.selected, Weapon::Missile);
        assert_eq!(save.cursor, 16);
        assert_eq!((save.ship_x, save.ship_y), (16, 3));
        assert_eq!(save.speed_level, 0);
        assert_eq!(save.weapon_upgrade_drop_countdown, 2);
        assert_eq!(save.scroll_accums.len(), 68);
        assert_eq!(save.entities.len(), 4);
        assert_eq!(save.entities[0].kind, 0x4A2F);
        assert_eq!(save.entities[0].health, 24);
        assert_eq!(save.effects.len(), 17);
    }

    #[test]
    fn the_dosbox_l4_save_decodes() {
        let save = SaveGame::decode(DOSBOX_L4_SAVE).expect("ground truth decodes");

        assert_eq!(save.level, Level::L4);
        assert_eq!(save.state.lives.get(), 3);
        assert_eq!(save.state.score, 0);
        assert_eq!(save.records.len(), 82);
        assert_eq!(save.cursor, 3);
        assert_eq!(save.weapon_upgrade_drop_countdown, 3);
        assert_eq!((save.ship_x, save.ship_y), (0, 19));
        assert_eq!(save.entities.len(), 3);
        assert_eq!(save.entities[0].kind, 0x3D86);
        assert_eq!(save.scroll_accums, vec![0x2BB0, 0x576, 0x91A, 0x1D20]);
    }

    #[test]
    fn the_dosbox_l6_save_decodes() {
        let save = SaveGame::decode(DOSBOX_L6_SAVE).expect("ground truth decodes");

        assert_eq!(save.level, Level::L6);
        assert_eq!(save.state.lives.get(), 2);
        assert_eq!(save.records.len(), 242);
        assert_eq!(save.cursor, 7);
        assert_eq!(save.entities.len(), 5);
        // The capture shot an obstacle: live damage in the entity buffer.
        assert_eq!(save.entities[4].health, 31_940);
        assert_eq!(save.effects.len(), 5);
        // The lingering spark is L6's own chaingun-spark descriptor, the
        // same value the per-WAD effect table carries.
        assert_eq!(save.effects[0].sprite, 0x3C90);
    }

    #[test]
    fn re_encoding_every_dosbox_save_is_idempotent() {
        for (name, bytes) in [
            ("l2-race", DOSBOX_RACE_SAVE),
            ("l1", DOSBOX_L1_SAVE),
            ("l3", DOSBOX_L3_SAVE),
            ("l5", DOSBOX_L5_SAVE),
            ("l7", DOSBOX_L7_SAVE),
            ("l4-race", DOSBOX_L4_SAVE),
            ("l6-race", DOSBOX_L6_SAVE),
        ] {
            let save = SaveGame::decode(bytes).expect("ground truth decodes");
            let encoded = save.encode();

            assert_eq!(encoded.len(), bytes.len(), "{name} length");
            assert_eq!(
                SaveGame::decode(&encoded).expect("re-decodes"),
                save,
                "{name} round trip"
            );
        }
    }

    #[test]
    fn the_wrong_length_is_rejected() {
        assert!(SaveGame::decode(&[2u8; 100]).is_err());
    }

    #[test]
    fn an_unknown_level_byte_is_rejected() {
        let mut bytes = sample().encode();
        bytes[0] = 9;

        assert!(SaveGame::decode(&bytes).is_err());
    }
}

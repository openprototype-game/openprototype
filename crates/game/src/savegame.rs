//! The `.psg` savegame codec.
//!
//! The port's save format IS the original's: `protosgN.psg` is a full
//! mid-level snapshot the level writes from its in-game menu (the writer at
//! LEVEL_2 file `0xb8da`, the loader at `0xb9d3`; see `re/savegame.md`).
//! A file is the level's variable block (whose first byte is the level
//! number) followed by both parities of each live-object buffer. Everything
//! is little-endian, no header, no padding between sections.
//!
//! This module maps the snapshot onto the port's semantic state. Fields the
//! port does not model yet are written as the original's idle values and
//! ignored on read; each is marked `TODO` at its offset. The freeze/dying
//! flags and the engine PRNG live outside the saved ranges, so a loaded
//! game always resumes through GET READY on a fresh clock seed, like the
//! original.
//!
//! Only the race family (levels 2/4/6) is implemented so far; the shooters
//! have per-WAD block lengths and AI tails still to be mapped.

use anyhow::{Result, bail};

use crate::level::slot::Record;
use crate::levels::Level;
use crate::spawns::{Effect, Entity, Shot};
use openprototype_core::game_state::Handoff;
use openprototype_core::{GameState, Lives, PerWeapon, SmartBombs, Weapon, WeaponLevel};

/// The race family's variable block: `cs:0xcb4..0x2895`.
const RACE_BLOCK_LEN: usize = 0x1be1;
/// The race spawn table inside the block: `cs:0xce6`, 867 8-byte records.
const RACE_TABLE_OFFSET: usize = 0xce6 - 0xcb4;
const RACE_TABLE_LEN: usize = 0x1b18;
/// The spawn cursor is stored as a cs pointer into the table (`cs:0x27fe`).
const RACE_TABLE_BASE: u16 = 0xce6;

/// One shot buffer (A/B/D): a count word and 99 16-byte records.
const SHOT_BUF_LEN: usize = 0x640;
const SHOT_RECORD_LEN: usize = 16;
/// One race entity buffer (C): a count word and 49 0x40-byte records.
const ENTITY_BUF_LEN: usize = 0xc80;
const ENTITY_RECORD_LEN: usize = 0x40;
/// One effects buffer (E): a count word and 399 16-byte records.
const EFFECT_BUF_LEN: usize = 0x1900;

/// A race `.psg`: block + A/A + B/B + C/C + D/D + E/E.
const RACE_FILE_LEN: usize =
    RACE_BLOCK_LEN + 4 * SHOT_BUF_LEN + 2 * ENTITY_BUF_LEN + 2 * SHOT_BUF_LEN + 2 * EFFECT_BUF_LEN;

/// A decoded savegame: the semantic state the port restores a level from.
///
/// In-flight player shots (the A buffer) and the fire system's transient
/// state (cooldowns, muzzle flash) are not carried yet; the weapons resume
/// idle, which costs at most a fraction of a second of fire state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveGame {
    pub level: Level,
    /// Score, lives, bombs, weapon levels, selection, invincibility, grace.
    pub state: GameState,
    /// The ISR-mutated spawn schedule (the head record's delay decays in
    /// place) and the consumer's position in it.
    pub records: Vec<Record>,
    pub cursor: usize,
    /// The orb/pebble-drop countdown (`cs:0x2848`).
    pub orb_drop_countdown: i32,
    /// The win flag (`cs:0xcc3`).
    pub level_end: bool,
    pub entities: Vec<Entity>,
    pub enemy_shots: Vec<Shot>,
    pub effects: Vec<Effect>,
    /// Ship x/y in pixels, the fly-in ramp counter, the roll frame.
    pub ship_x: i32,
    pub ship_y: i32,
    pub ship_ramp: i32,
    pub ship_roll: i32,
    /// The four parallax scroll accumulators (`cs:0x2804..`, dwords).
    pub scroll_accums: [u32; 4],
    /// The race speed level (`cs:0x2852`, 0..=0x20).
    pub speed_level: u16,
}

impl SaveGame {
    /// The carried payload embedded in the snapshot, for code that wants
    /// the handoff view of it.
    pub fn handoff(&self) -> Handoff {
        self.state.handoff()
    }

    /// Encode as the original race-family `.psg`. Panics if `level` is not
    /// a race (the shooter codec is not implemented yet).
    pub fn encode(&self) -> Vec<u8> {
        assert!(
            matches!(self.level, Level::L2 | Level::L4 | Level::L6),
            "only the race codec is implemented"
        );

        let mut bytes = vec![0u8; RACE_FILE_LEN];
        self.encode_race_block(&mut bytes[..RACE_BLOCK_LEN]);

        let mut at = RACE_BLOCK_LEN;

        // A: player shots, both parities. Not carried yet; empty lists.
        at += 2 * SHOT_BUF_LEN;

        // B: enemy shots, both parities (identical copies are consistent;
        // the engine reads the parity the block names, which we write as 0).
        for _ in 0..2 {
            encode_shots(&mut bytes[at..at + SHOT_BUF_LEN], &self.enemy_shots);
            at += SHOT_BUF_LEN;
        }

        // C: entities, both parities.
        for _ in 0..2 {
            encode_entities(&mut bytes[at..at + ENTITY_BUF_LEN], &self.entities);
            at += ENTITY_BUF_LEN;
        }

        // D: the fire-staging buffer, merged into A every frame; transient,
        // saved empty.
        at += 2 * SHOT_BUF_LEN;

        // E: effects, both parities.
        for _ in 0..2 {
            encode_effects(&mut bytes[at..at + EFFECT_BUF_LEN], &self.effects);
            at += EFFECT_BUF_LEN;
        }

        bytes
    }

    /// Decode a race-family `.psg`.
    pub fn decode(bytes: &[u8]) -> Result<SaveGame> {
        if bytes.len() != RACE_FILE_LEN {
            bail!(
                "savegame is {} bytes, the race format is {RACE_FILE_LEN} \
                 (the shooter formats are not supported yet)",
                bytes.len()
            );
        }

        let block = &bytes[..RACE_BLOCK_LEN];
        let level = match block[0] {
            2 => Level::L2,
            4 => Level::L4,
            6 => Level::L6,
            other => bail!("savegame is for level {other}, not a race"),
        };

        let word = |at: usize| u16::from_le_bytes([block[at], block[at + 1]]);
        let dword = |at: usize| {
            u32::from_le_bytes([block[at], block[at + 1], block[at + 2], block[at + 3]])
        };

        // The parity words select which copy of each pair is current.
        let parity_abc = usize::from(word(0xcc5 - 0xcb4)) / 2;
        let parity_e = usize::from(word(0xccf - 0xcb4)) / 2;

        let selected = match word(0xcb7 - 0xcb4) {
            2 => Weapon::Burning,
            3 => Weapon::Plasma,
            4 => Weapon::Missile,
            _ => Weapon::Multishot,
        };
        let mut weapons = PerWeapon::splat(WeaponLevel::new(0));

        for (index, weapon) in Weapon::ALL.iter().enumerate() {
            let fill = word(0xcb9 - 0xcb4 + index * 2);
            *weapons.get_mut(*weapon) = WeaponLevel::new((fill / 8) as u8);
        }

        let state = GameState {
            score: dword(0x2873 - 0xcb4),
            lives: Lives::new(block[0x284b - 0xcb4]),
            smart_bombs: SmartBombs::new(word(0xcc1 - 0xcb4) as u8),
            weapons,
            selected,
            invincible_ticks: word(0x284c - 0xcb4),
            contact_grace_ticks: word(0x284e - 0xcb4),
        };

        let table = &block[RACE_TABLE_OFFSET..RACE_TABLE_OFFSET + RACE_TABLE_LEN];
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

        let cursor_pointer = word(0x27fe - 0xcb4);
        let cursor = usize::from(cursor_pointer.saturating_sub(RACE_TABLE_BASE)) / 8;

        let mut scroll_accums = [0u32; 4];

        for (index, accum) in scroll_accums.iter_mut().enumerate() {
            *accum = dword(0x2804 - 0xcb4 + index * 4);
        }

        let buffer = |index: usize, len: usize| {
            let mut at = RACE_BLOCK_LEN;
            let sizes = [
                SHOT_BUF_LEN,
                SHOT_BUF_LEN,
                SHOT_BUF_LEN,
                SHOT_BUF_LEN,
                ENTITY_BUF_LEN,
                ENTITY_BUF_LEN,
                SHOT_BUF_LEN,
                SHOT_BUF_LEN,
                EFFECT_BUF_LEN,
                EFFECT_BUF_LEN,
            ];

            for size in &sizes[..index] {
                at += size;
            }

            &bytes[at..at + len]
        };

        let enemy_shots = decode_shots(buffer(2 + parity_abc, SHOT_BUF_LEN));
        let entities = decode_entities(buffer(4 + parity_abc, ENTITY_BUF_LEN));
        let effects = decode_effects(buffer(8 + parity_e, EFFECT_BUF_LEN));

        Ok(SaveGame {
            level,
            state,
            records,
            cursor,
            orb_drop_countdown: i32::from(word(0x2848 - 0xcb4) as i16),
            level_end: block[0xcc3 - 0xcb4] == 1,
            entities,
            enemy_shots,
            effects,
            ship_x: i32::from(word(0x2826 - 0xcb4) as i16),
            ship_y: i32::from(word(0x2828 - 0xcb4) as i16),
            ship_ramp: i32::from(word(0x2824 - 0xcb4) as i16),
            ship_roll: i32::from(word(0x2846 - 0xcb4) as i16) / 0x12,
            scroll_accums,
            speed_level: word(0x2852 - 0xcb4),
        })
    }

    /// Fill the race variable block. Unmodeled engine scratch is left at
    /// the original's idle values (zeros, or the marked literals).
    fn encode_race_block(&self, block: &mut [u8]) {
        fn put_word(block: &mut [u8], at: usize, value: u16) {
            block[at - 0xcb4..at - 0xcb4 + 2].copy_from_slice(&value.to_le_bytes());
        }

        // Weapons: firing resolves on the first unfreeze; storing the
        // selected weapon (or 0 on an empty slot) matches the original's
        // resolve. TODO: the firing freeze across a held burst is lost.
        let firing = match self.state.active_weapon() {
            openprototype_core::ActiveWeapon::Chaingun => 0,
            openprototype_core::ActiveWeapon::Selected(weapon) => weapon_index(weapon),
        };
        put_word(block, 0xcb5, firing);
        put_word(block, 0xcb7, weapon_index(self.state.selected));

        for (index, weapon) in Weapon::ALL.iter().enumerate() {
            let fill = u16::from(self.state.level(*weapon).get()) * 8;
            put_word(block, 0xcb9 + index * 2, fill);
        }

        put_word(block, 0xcc1, u16::from(self.state.smart_bombs.get()));
        block[0xcc3 - 0xcb4] = u8::from(self.level_end);
        // Parities: both copies are written identically, current = 0.
        // TODO at 0xcc7..0xccc: fire cooldown and muzzle-flash state
        // (ground truth shows the firing weapon's authored cooldown here).
        // TODO at 0xcde/0xce4: spawn-clock scratch seen as 01/03 live.

        let table_end = RACE_TABLE_OFFSET + self.records.len() * 8;

        for (index, record) in self.records.iter().enumerate() {
            let at = RACE_TABLE_OFFSET + index * 8;
            block[at..at + 2].copy_from_slice(&record.delay.to_le_bytes());
            block[at + 2..at + 4].copy_from_slice(&record.sprite.to_le_bytes());
            block[at + 4..at + 6].copy_from_slice(&record.health.to_le_bytes());
            block[at + 6..at + 8].copy_from_slice(&record.spawn_row.to_le_bytes());
        }

        // The zero-sprite terminator after the last record.
        let _ = table_end;

        put_word(block, 0x27fe, RACE_TABLE_BASE + (self.cursor as u16) * 8);

        // Scroll bookkeeping: the static list pointer/count, then the live
        // accumulators and their per-tick constants.
        put_word(block, 0x2800, 0x2804);
        put_word(block, 0x2802, 4);

        for (index, accum) in self.scroll_accums.iter().enumerate() {
            let at = 0x2804 - 0xcb4 + index * 4;
            block[at..at + 4].copy_from_slice(&accum.to_le_bytes());
        }

        // The per-tick scroll constants are dwords (ground truth: values at
        // stride 4 from 0x2814).
        for (index, constant) in [0x30u32, 6, 0xa, 0x20].iter().enumerate() {
            block[0x2814 - 0xcb4 + index * 4..0x2814 - 0xcb4 + index * 4 + 4]
                .copy_from_slice(&constant.to_le_bytes());
        }

        // The ship-trail history rings (x at 0x282a, y at 0x2838, 7 words
        // each); a save parked on the current position is consistent.
        for slot in 0..7 {
            block[0x282a - 0xcb4 + slot * 2..0x282a - 0xcb4 + slot * 2 + 2]
                .copy_from_slice(&(self.ship_x as u16).to_le_bytes());
            block[0x2838 - 0xcb4 + slot * 2..0x2838 - 0xcb4 + slot * 2 + 2]
                .copy_from_slice(&(self.ship_y as u16).to_le_bytes());
        }

        put_word(block, 0x2824, self.ship_ramp as u16);
        put_word(block, 0x2826, self.ship_x as u16);
        put_word(block, 0x2828, self.ship_y as u16);
        put_word(block, 0x2846, (self.ship_roll * 0x12) as u16);
        put_word(block, 0x2848, self.orb_drop_countdown as u16);
        block[0x284b - 0xcb4] = self.state.lives.get();
        put_word(block, 0x284c, self.state.invincible_ticks);
        put_word(block, 0x284e, self.state.contact_grace_ticks);
        put_word(block, 0x2850, self.speed_level * 0x50);
        put_word(block, 0x2852, self.speed_level);

        // Score, the digit caches as the in-level load path reseeds them
        // (0x0b = blank, forcing a full redraw), and the extra-life decade
        // threshold.
        block[0x2873 - 0xcb4..0x2873 - 0xcb4 + 4].copy_from_slice(&self.state.score.to_le_bytes());
        block[0x286d - 0xcb4..0x2873 - 0xcb4].fill(0x0b);

        put_word(block, 0x2877, (self.state.score / 10_000) as u16);

        // The in-level-load housekeeping byte (saved as 1 in ground truth)
        // and the HUD redraw flags as a running game carries them.
        block[0x2879 - 0xcb4] = 1;
        block[0x287c - 0xcb4] = 1;
        // TODO at 0x2856..0x2862, 0x2865/0x2866, 0x2869/0x286b, 0x287d/
        // 0x287f: engine scratch observed live; unmapped.

        block[0] = match self.level {
            Level::L2 => 2,
            Level::L4 => 4,
            Level::L6 => 6,
            _ => unreachable!("the race codec only encodes races"),
        };
    }
}

fn weapon_index(weapon: Weapon) -> u16 {
    match weapon {
        Weapon::Multishot => 1,
        Weapon::Burning => 2,
        Weapon::Plasma => 3,
        Weapon::Missile => 4,
    }
}

/// Write a shot list as a count word plus 16-byte records.
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
        // TODO at +0xa/+0xb/+0xe: the enemy-shot sizes and damage; the
        // port's ship hit test does not read them from the record yet.
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

/// Write the entity list as a count word plus 0x40-byte records (the
/// entity layout from `re/spawn-consumer.md`).
fn encode_entities(buffer: &mut [u8], entities: &[Entity]) {
    let count = entities.len().min((ENTITY_BUF_LEN - 2) / ENTITY_RECORD_LEN);
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
        // TODO at +0x1b: the fire-pattern word (0xffff = none); the port
        // drives enemy fire from the AI functions directly.
        put_word(0x1b, 0xffff);
        put_word(0x20, entity.tick);
        put_word(0x22, entity.phase_a);
        put_word(0x24, entity.phase_b);
        put_word(0x26, entity.save_y as u16);
        put_word(0x28, entity.save_x as u16);
        put_word(0x2a, entity.counter);

        buffer[at + 0x1a] = u8::from(entity.seen);
        buffer[at + 0x1f] = entity.anim;

        for (box_index, hitbox) in entity.hitboxes.iter().enumerate() {
            buffer[at + 8 + box_index * 4..at + 8 + box_index * 4 + 4].copy_from_slice(hitbox);
        }
    }
}

fn decode_entities(buffer: &[u8]) -> Vec<Entity> {
    let count = usize::from(u16::from_le_bytes([buffer[0], buffer[1]]))
        .min((ENTITY_BUF_LEN - 2) / ENTITY_RECORD_LEN);

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
                seen: buffer[at + 0x1a] != 0,
                anim: buffer[at + 0x1f],
                tick: word(0x20),
                // The death handler reads the debris template from the
                // type descriptor, not the record; the loader re-derives it.
                debris: 0,
                hitboxes,
                phase_a: word(0x22),
                phase_b: word(0x24),
                save_y: signed(0x26),
                save_x: signed(0x28),
                counter: word(0x2a),
            }
        })
        .collect()
}

/// Write the effects list as a count word plus 16-byte records.
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
        buffer[at + 0xa] = effect.phase;
        buffer[at + 0xb..at + 0xd].copy_from_slice(&effect.delay.to_le_bytes());
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
                phase: buffer[at + 0xa],
                delay: word(0xb),
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
                    sprite: 0x3c3c,
                    health: 32000,
                    spawn_row: 12,
                },
                Record {
                    delay: 400,
                    sprite: 0x3e1c,
                    health: 0x14,
                    spawn_row: 209,
                },
            ],
            cursor: 1,
            orb_drop_countdown: 4,
            level_end: false,
            entities: vec![Entity {
                sprite: 0x3c5a,
                kind: 0x3c5a,
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
                    [0x0c, 0x04, 0x38, 0x19],
                    [0x1c, 0x02, 0x2f, 0x1e],
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
                sprite: 0x3b48,
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
            scroll_accums: [0x1234, 0x5678, 0x9abc, 0xdef0],
            speed_level: 0x10,
        }
    }

    #[test]
    fn the_race_file_has_the_original_length() {
        assert_eq!(sample().encode().len(), 0x8c61);
    }

    #[test]
    fn the_level_byte_leads_the_file() {
        assert_eq!(sample().encode()[0], 2);
    }

    #[test]
    fn a_round_trip_preserves_the_modeled_state() {
        let save = sample();
        let decoded = SaveGame::decode(&save.encode()).expect("decodes");

        assert_eq!(decoded, save);
    }

    /// A real save created by the original engine in DOSBox (slot 1 of the
    /// fixture capture session: L2 entered with a 25,000-point carry).
    const DOSBOX_RACE_SAVE: &[u8] = include_bytes!("../tests/fixtures/l2-race.psg");

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
        assert_eq!(save.orb_drop_countdown, 3);
        assert!(!save.level_end);
        assert_eq!((save.ship_x, save.ship_y), (120, 23));
        assert_eq!(save.ship_ramp, 10);
        assert_eq!(save.ship_roll, 8);
        assert_eq!(save.speed_level, 0);
        assert_eq!(save.scroll_accums, [0x30c0, 0x618, 0xa28, 0x2080]);
        // Five live obstacles, two carrying the player's symbolic shot
        // damage, including the table's stack-of-three thin bars.
        assert_eq!(save.entities.len(), 5);
        assert_eq!(save.entities[0].kind, 0x3e1c);
        assert_eq!(save.entities[0].health, 31_964);
        assert_eq!(save.entities[1].kind, 0x3f2a);
        assert_eq!(save.entities[2].kind, 0x3eb2);
        assert_eq!(save.entities[2].health, 31_928);
        assert_eq!(save.entities[4].kind, 0x3eb2);
        assert!(save.enemy_shots.is_empty());
        assert!(save.effects.is_empty());
    }

    #[test]
    fn re_encoding_the_dosbox_save_is_idempotent() {
        let save = SaveGame::decode(DOSBOX_RACE_SAVE).expect("ground truth decodes");
        let encoded = save.encode();

        assert_eq!(encoded.len(), DOSBOX_RACE_SAVE.len());
        assert_eq!(SaveGame::decode(&encoded).expect("re-decodes"), save);
    }

    #[test]
    fn the_shooter_saves_are_rejected_until_their_codec_lands() {
        let bytes = include_bytes!("../tests/fixtures/l1.psg");

        assert!(SaveGame::decode(bytes).is_err());
    }

    #[test]
    fn the_wrong_length_is_rejected() {
        assert!(SaveGame::decode(&[0u8; 100]).is_err());
    }

    #[test]
    fn a_shooter_level_byte_is_rejected_for_now() {
        let mut bytes = sample().encode();
        bytes[0] = 1;

        assert!(SaveGame::decode(&bytes).is_err());
    }
}

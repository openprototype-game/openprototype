//! The enemy/pickup spawn layer: scheduling, live entities, and their render.
//!
//! Mirrors the original's consumer (see `reference/formats/level-layout.md`,
//! "The spawn consumer", and `re/spawn-consumer.md`): the timer ISR decrements
//! the head record's delay once per tick and the update loop pulls every due
//! record into a live entity, placing it from the level's spawn-position
//! table. Entities draw in buffer order through the catalog-cell blitter; the
//! port assembles each sprite from its descriptor over the clip-header catalog
//! and blits it whole.

mod ai_l1;

use std::collections::HashMap;

use crate::assets::{OverlaySprite, directory_sprite};
use crate::level::prng::{EngineRng, clock_seed};
use crate::level::slot::Record;
use crate::levels::SpawnAi;
use crate::playfield;
use openprototype_core::framebuffer::Framebuffer;
use prototype_formats::bin::SpriteSheet;

/// The live-entity cap, the original's hard error bound (`0x18`).
const MAX_ENTITIES: usize = 24;

/// The off-screen cull bounds in 12.4 fixed point, from the update loop's
/// signed compares (both ends inclusive): x in [-1280, 4608], y in
/// [-960, 2560].
const CULL_X: std::ops::RangeInclusive<i32> = -0x500..=0x1200;
const CULL_Y: std::ops::RangeInclusive<i32> = -0x3c0..=0xa00;

/// One row of a level's spawn-position table: where a spawn enters the
/// playfield and how it moves.
///
/// `mode` 0 runs the per-type AI function `arg`; a nonzero mode is the
/// (relocated) segment of a `{dx, dy, danim}` path table that `arg` indexes
/// into.
#[derive(Clone, Copy)]
pub struct SpawnRow {
    pub x: i32,
    pub y: i32,
    pub mode: u16,
    pub arg: u16,
}

/// Decodes a WAD's spawn-position table: `rows` 8-byte rows at file offset
/// `table`.
pub fn decode_rows(wad: &[u8], table: usize, rows: usize) -> anyhow::Result<Vec<SpawnRow>> {
    let end = table + rows * 8;

    if wad.len() < end {
        anyhow::bail!("WAD is {} bytes, spawn table needs {end}", wad.len());
    }

    Ok((0..rows)
        .map(|row| {
            let at = table + row * 8;
            let word = |k: usize| u16::from_le_bytes([wad[at + k * 2], wad[at + k * 2 + 1]]);
            SpawnRow {
                x: i32::from(word(0) as i16),
                y: i32::from(word(1) as i16),
                mode: word(2),
                arg: word(3),
            }
        })
        .collect())
}

/// One live enemy or pickup, the port's view of the original's 0x40-byte
/// entity (field offsets per `re/l1-ai-functions.md`).
pub struct Entity {
    /// The current sprite descriptor cs-pointer (the animation frame).
    pub sprite: u16,
    /// The rest/type descriptor pointer (identifies the species).
    pub kind: u16,
    /// Position in 12.4 fixed point, camera-inclusive buffer coordinates.
    pub x: i32,
    pub y: i32,
    /// Movement source from the spawn row: mode 0 = AI function `arg`.
    pub mode: u16,
    pub arg: u16,
    /// Hit points (entity +0x18); a negative value removes the entity (the
    /// boss's form-2 self-destruct writes -1).
    pub health: i32,
    /// Set once the entity has been on screen; the off-screen cull only
    /// applies after that (entity +0x1a).
    pub seen: bool,
    /// The animation tick counter (entity +0x1f).
    pub anim: u8,
    /// The per-life tick counter (entity +0x20).
    pub tick: u16,
    /// Orbit/path phase words (entity +0x22/+0x24).
    pub phase_a: u16,
    pub phase_b: u16,
    /// Stored orbit-center position (entity +0x26/+0x28).
    pub save_y: i32,
    pub save_x: i32,
}

/// One enemy shot (the original's 16-byte buffer-B record): position and
/// velocity in 12.4 per sub-step.
pub struct Shot {
    pub sprite: u16,
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
}

/// The enemy shots' cull bounds (the move loop's signed compares, both
/// exclusive): x in (-0x200, 0x1200), y in (0, 0xa00).
const SHOT_X: std::ops::Range<i32> = -0x1ff..0x1200;
const SHOT_Y: std::ops::Range<i32> = 1..0xa00;

/// The spawn schedule, live entities, and enemy shots for a running level.
pub struct Spawns {
    /// The level's spawn records, in spawn order.
    records: Vec<Record>,
    /// Index of the next record to spawn.
    cursor: usize,
    /// The head record's remaining delay (the original decays the record's
    /// word in place; the port keeps the buffer pristine and counts here).
    countdown: i32,
    pub entities: Vec<Entity>,
    pub shots: Vec<Shot>,
    /// Which per-level AI set drives mode-0 entities, when transcribed.
    ai: Option<SpawnAi>,
    /// The engine PRNG the AI functions draw from (shooter fire chances).
    rng: EngineRng,
    /// The boss's engine globals.
    boss: ai_l1::BossState,
    /// Sprites assembled from descriptors, cached by descriptor pointer
    /// (`None` caches an unresolvable descriptor).
    sprites: HashMap<u16, Option<OverlaySprite>>,
}

impl Spawns {
    /// Builds the schedule over a generated (or static) record buffer.
    pub fn new(records: Vec<Record>, ai: Option<SpawnAi>) -> Self {
        let countdown = records.first().map_or(0, |record| i32::from(record.delay));

        Self {
            records,
            cursor: 0,
            countdown,
            entities: Vec::new(),
            shots: Vec::new(),
            ai,
            rng: EngineRng::new(clock_seed()),
            boss: ai_l1::BossState::default(),
            sprites: HashMap::new(),
        }
    }

    /// One PIT tick of the spawn clock: decrement the head delay and pull
    /// every due record into a live entity.
    ///
    /// The original splits this between the ISR (the decrement) and the
    /// update-loop tail (the pull); the pull chains while consecutive records
    /// are due, so zero-delay records spawn together. The entity cap drops
    /// overflow spawns (the original treats overflow as a fatal error; the
    /// port degrades gracefully).
    pub fn tick(&mut self, rows: &[SpawnRow]) {
        if self.cursor >= self.records.len() {
            return;
        }

        self.countdown -= 1;

        while self.cursor < self.records.len() && self.countdown <= 0 {
            let record = self.records[self.cursor];

            if record.sprite == 0 {
                break;
            }

            self.cursor += 1;
            self.countdown = self
                .records
                .get(self.cursor)
                .map_or(0, |next| i32::from(next.delay));

            let Some(row) = rows.get(usize::from(record.spawn_row)) else {
                continue;
            };

            if self.entities.len() >= MAX_ENTITIES {
                continue;
            }

            self.entities.push(Entity {
                sprite: record.sprite,
                kind: record.sprite,
                x: row.x << 4,
                y: row.y << 4,
                mode: row.mode,
                arg: row.arg,
                health: i32::from(record.health),
                seen: false,
                anim: 0,
                tick: 0,
                phase_a: 0,
                phase_b: 0,
                save_y: 0,
                save_x: 0,
            });
        }
    }

    /// One movement sub-step for every entity and shot, then the culls.
    ///
    /// The original runs this `elapsed_ticks` times per rendered frame (the
    /// catch-up stepping); the caller loops accordingly. `player` is the
    /// ship's pixel position (the aimed shooters lead on it).
    pub fn step_movement(&mut self, wad: &[u8], player: (i32, i32)) {
        if let Some(SpawnAi::L1) = self.ai {
            let mut context = ai_l1::AiContext {
                wad,
                rng: &mut self.rng,
                player_x: player.0,
                player_y: player.1,
                shots: &mut self.shots,
                boss: &mut self.boss,
            };

            for entity in &mut self.entities {
                if entity.mode == 0 {
                    ai_l1::step(entity, &mut context);
                }
                // TODO: mode != 0 follows a {dx, dy, dsprite} path table at
                // the mode segment; no LEVEL_1 row uses it.
            }
        }

        // The cull only removes entities that have already been on screen
        // (the original's seen flag), so right-edge spawns survive their
        // off-screen entry. A negative health is the boss's removal sentinel
        // (combat damage is not wired yet).
        self.entities.retain_mut(|entity| {
            if entity.health < 0 {
                return false;
            }

            let in_bounds = CULL_X.contains(&entity.x) && CULL_Y.contains(&entity.y);

            if in_bounds {
                entity.seen = true;
            }

            in_bounds || !entity.seen
        });

        for shot in &mut self.shots {
            shot.x += shot.vx;
            shot.y += shot.vy;
        }

        self.shots
            .retain(|shot| SHOT_X.contains(&shot.x) && SHOT_Y.contains(&shot.y));
    }

    /// The schedule position, for tests: `(next record index, head countdown)`.
    #[cfg(test)]
    fn cursor_state(&self) -> (usize, i32) {
        (self.cursor, self.countdown)
    }

    /// Draws every live entity in buffer order (the original has no depth
    /// sort).
    ///
    /// `wad`/`cs_base` resolve descriptor pointers (`file = ptr + cs_base`)
    /// and `catalog` supplies the cells; sprites are cached per descriptor.
    pub fn render(
        &mut self,
        wad: &[u8],
        cs_base: usize,
        catalog: &SpriteSheet,
        frame: &mut Framebuffer,
        camera: i32,
    ) {
        for entity in &self.entities {
            // The descriptor is the same {ncells, w, h, cell} shape as the
            // shield/fire directory records, so the same assembler applies.
            let sprite = self.sprites.entry(entity.sprite).or_insert_with(|| {
                directory_sprite(wad, catalog, usize::from(entity.sprite) + cs_base).ok()
            });

            if let Some(sprite) = sprite {
                frame.blit_transparent(
                    &sprite.pixels,
                    sprite.size,
                    playfield::LEFT + (entity.x >> 4),
                    (entity.y >> 4) - camera,
                );
            }
        }

        // Enemy shots draw after the entities (the original's buffer order).
        for shot in &self.shots {
            let sprite = self.sprites.entry(shot.sprite).or_insert_with(|| {
                directory_sprite(wad, catalog, usize::from(shot.sprite) + cs_base).ok()
            });

            if let Some(sprite) = sprite {
                frame.blit_transparent(
                    &sprite.pixels,
                    sprite.size,
                    playfield::LEFT + (shot.x >> 4),
                    (shot.y >> 4) - camera,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(delay: u16, spawn_row: u16) -> Record {
        Record {
            delay,
            sprite: 0x3308,
            health: 100,
            spawn_row,
        }
    }

    fn rows() -> Vec<SpawnRow> {
        vec![
            SpawnRow {
                x: 288,
                y: 10,
                mode: 0,
                arg: 0,
            },
            SpawnRow {
                x: -20,
                y: 30,
                mode: 0,
                arg: 1,
            },
        ]
    }

    #[test]
    fn pulls_due_records_and_chains_zero_delays() {
        // Delays 3, 0, 2: the first two spawn together on tick 3, the third
        // two ticks later.
        let mut spawns = Spawns::new(vec![record(3, 0), record(0, 1), record(2, 0)], None);
        let rows = rows();

        spawns.tick(&rows);
        spawns.tick(&rows);
        assert_eq!(spawns.entities.len(), 0);

        spawns.tick(&rows);
        assert_eq!(spawns.entities.len(), 2);
        assert_eq!(spawns.cursor_state(), (2, 2));
        assert_eq!(
            (spawns.entities[0].x, spawns.entities[0].y),
            (288 << 4, 10 << 4)
        );
        assert_eq!(
            (spawns.entities[1].x, spawns.entities[1].y),
            (-20 << 4, 30 << 4)
        );

        spawns.tick(&rows);
        spawns.tick(&rows);
        assert_eq!(spawns.entities.len(), 3);
    }

    #[test]
    fn caps_the_entity_list() {
        let records = (0..30).map(|_| record(0, 0)).collect();
        let mut spawns = Spawns::new(records, None);

        spawns.tick(&rows());
        assert_eq!(spawns.entities.len(), MAX_ENTITIES);
    }

    #[test]
    fn culls_out_of_bounds_entities() {
        let mut spawns = Spawns::new(vec![record(1, 0)], None);

        spawns.tick(&rows());
        assert_eq!(spawns.entities.len(), 1);

        // The bounds are inclusive: -0x500 survives, one step past it culls.
        spawns.entities[0].x = -0x500;
        spawns.step_movement(&[], (0, 0));
        assert_eq!(spawns.entities.len(), 1);

        spawns.entities[0].x = -0x501;
        spawns.step_movement(&[], (0, 0));
        assert!(spawns.entities.is_empty());
    }

    #[test]
    #[cfg_attr(not(feature = "disc-tests"), ignore = "requires the disc image")]
    fn decodes_the_l1_spawn_table() {
        use crate::levels::Level;
        use prototype_disc::{AssetSource, DiscImage};

        let disc = DiscImage::open_default().expect("disc image");
        let data = Level::L1.data();
        let wad = disc.read(data.wad).expect("reading LEVEL_1.WAD");
        let positions = data.spawn_positions.expect("L1 spawn positions");
        let rows = decode_rows(&wad, positions.table, positions.rows).expect("decoding rows");

        assert_eq!(rows.len(), 76);
        // Row 0: the first asteroid lane, right edge.
        assert_eq!(
            (rows[0].x, rows[0].y, rows[0].mode, rows[0].arg),
            (288, 10, 0, 0)
        );
        // Row 71: the first mid-screen pickup spot.
        assert_eq!((rows[71].x, rows[71].y, rows[71].arg), (170, 80, 5));
        // Every L1 row is an AI-function spawn.
        assert!(rows.iter().all(|row| row.mode == 0));
    }
}

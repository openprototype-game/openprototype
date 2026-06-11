//! The enemy/pickup spawn layer: scheduling, live entities, and their render.
//!
//! Mirrors the original's consumer (see `reference/formats/level-layout.md`,
//! "The spawn consumer", and `re/spawn-consumer.md`): the timer ISR decrements
//! the head record's delay once per tick and the update loop pulls every due
//! record into a live entity, placing it from the level's spawn-position
//! table. Entities draw in buffer order through the catalog-cell blitter; the
//! port assembles each sprite from its descriptor over the clip-header catalog
//! and blits it whole.

use std::collections::HashMap;

use crate::assets::{OverlaySprite, directory_sprite};
use crate::level::slot::Record;
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
/// entity.
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
    /// The animation tick counter (entity byte +0x17).
    pub anim_tick: u8,
    /// The health / per-type state word (entity word +0x18; several AI
    /// functions reuse it as a timer).
    pub state: i32,
}

/// The spawn schedule and live-entity list for a running level.
pub struct Spawns {
    /// The level's spawn records, in spawn order.
    records: Vec<Record>,
    /// Index of the next record to spawn.
    cursor: usize,
    /// The head record's remaining delay (the original decays the record's
    /// word in place; the port keeps the buffer pristine and counts here).
    countdown: i32,
    pub entities: Vec<Entity>,
    /// Sprites assembled from descriptors, cached by descriptor pointer
    /// (`None` caches an unresolvable descriptor).
    sprites: HashMap<u16, Option<OverlaySprite>>,
}

impl Spawns {
    /// Builds the schedule over a generated (or static) record buffer.
    pub fn new(records: Vec<Record>) -> Self {
        let countdown = records.first().map_or(0, |record| i32::from(record.delay));

        Self {
            records,
            cursor: 0,
            countdown,
            entities: Vec::new(),
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
                anim_tick: 0,
                state: i32::from(record.health),
            });
        }
    }

    /// One movement sub-step for every entity, then the off-screen cull.
    ///
    /// The original runs this `elapsed_ticks` times per rendered frame (the
    /// catch-up stepping); the caller loops accordingly.
    pub fn step_movement(&mut self) {
        // TODO: per-type AI functions (mode 0) and path tables (mode != 0);
        // transcription in progress (re/l1-ai-functions.md). Until then the
        // entities hold their spawn positions.

        self.entities
            .retain(|entity| CULL_X.contains(&entity.x) && CULL_Y.contains(&entity.y));
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
        let mut spawns = Spawns::new(vec![record(3, 0), record(0, 1), record(2, 0)]);
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
        let mut spawns = Spawns::new(records);

        spawns.tick(&rows());
        assert_eq!(spawns.entities.len(), MAX_ENTITIES);
    }

    #[test]
    fn culls_out_of_bounds_entities() {
        let mut spawns = Spawns::new(vec![record(1, 0)]);

        spawns.tick(&rows());
        assert_eq!(spawns.entities.len(), 1);

        // The bounds are inclusive: -0x500 survives, one step past it culls.
        spawns.entities[0].x = -0x500;
        spawns.step_movement();
        assert_eq!(spawns.entities.len(), 1);

        spawns.entities[0].x = -0x501;
        spawns.step_movement();
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

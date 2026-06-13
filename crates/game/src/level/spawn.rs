//! Where a level's enemy/pickup spawn records come from.
//!
//! Two mechanisms, one record format (see `reference/formats/level-layout.md`):
//! the generated levels (1, 3, 5, 7) run their layout script through the slot
//! interpreter at load, the race levels (2, 4, 6) bake a static table into the
//! WAD. [`SpawnSource::records`] resolves either into the same [`Record`]
//! buffer.

use super::prng::EngineRng;
use super::slot::{PostOp, Record, Step, generate};

/// The static table's fixed capacity in 8-byte slots, the header included.
const STATIC_SLOTS: usize = 244;

/// A level's spawn-placement source.
#[derive(Clone, Copy)]
pub enum SpawnSource {
    /// Built at load by the level's layout script (levels 1, 3, 5, 7).
    ///
    /// The script and post-pass are the validated transcriptions in
    /// `level_<n>.rs`; the PRNG seed makes the scatter vary per play.
    Generated {
        script: fn() -> Vec<Step>,
        post_pass: Option<fn() -> Vec<PostOp>>,
    },
    /// A static table baked into the WAD (race levels 2, 4, 6).
    ///
    /// `table` is the file offset of the first record. The records are the
    /// shooter consumer's exact shape (`{delay, sprite, health, spawn_row}`,
    /// see `re/race-mode.md`); a record with sprite 0 terminates the run.
    StaticTable { table: usize },
}

impl SpawnSource {
    /// Resolves the source into the level's spawn-record buffer.
    ///
    /// `wad` is the level's WAD image (read by [`StaticTable`]
    /// (SpawnSource::StaticTable)); `rng` is the level's single engine
    /// stream (seeded once per entry from
    /// [`clock_seed`](super::prng::clock_seed)): the layout draws advance
    /// it, and the star scatter and play draws continue from there, like
    /// the original's one-generator model.
    pub fn records(self, wad: &[u8], rng: &mut EngineRng) -> Vec<Record> {
        match self {
            SpawnSource::Generated { script, post_pass } => {
                let post = post_pass.map_or_else(Vec::new, |build| build());
                generate(&script(), &post, rng)
            }
            SpawnSource::StaticTable { table } => static_records(wad, table),
        }
    }
}

/// Reads the populated run of a race WAD's static spawn table.
///
/// The records are the shooter consumer's `{delay, sprite, health, row}`
/// shape; the run ends at the first record whose sprite word is zero (the
/// `{20, 0, 0, 0}` terminator shared by all three race WADs). The last live
/// record is the finish entity (`{400, sprite, 0x14, 209}`).
fn static_records(wad: &[u8], table: usize) -> Vec<Record> {
    let mut out = Vec::new();

    for slot in 0..STATIC_SLOTS {
        let bytes = &wad[table + slot * 8..table + (slot + 1) * 8];
        let word = |k: usize| u16::from_le_bytes([bytes[k * 2], bytes[k * 2 + 1]]);

        if word(1) == 0 {
            break;
        }

        out.push(Record {
            delay: word(0),
            sprite: word(1),
            health: word(2),
            spawn_row: word(3),
        });
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::level_3;
    use crate::levels::Level;

    #[test]
    fn generated_arm_runs_the_script_and_post_pass() {
        let source = Level::L3.data().spawns;
        let records = source.records(&[], &mut EngineRng::new(0x1a94));

        let direct = generate(
            &level_3::script(),
            &level_3::post_pass(),
            &mut EngineRng::new(0x1a94),
        );
        assert_eq!(records, direct);
    }

    #[test]
    #[cfg_attr(not(feature = "disc-tests"), ignore = "requires the disc image")]
    fn parses_the_race_wad_tables() {
        use prototype_disc::{AssetSource, DiscImage};

        let disc = DiscImage::open_default().expect("disc image");

        for (level, expected_count, first_delay, first_row) in [
            (Level::L2, 67, 200, 21),
            (Level::L4, 82, 200, 17),
            (Level::L6, 242, 100, 9),
        ] {
            let data = level.data();
            let wad = disc.read(data.wad).expect("reading the race WAD");
            assert!(
                matches!(data.spawns, SpawnSource::StaticTable { .. }),
                "race level without a static table"
            );

            let records = data.spawns.records(&wad, &mut EngineRng::new(1));
            assert_eq!(records.len(), expected_count, "{}", data.wad);

            // The head record pins the table base: the opening obstacle
            // with the races' symbolic 32000 health.
            let first = records.first().expect("populated run");
            assert_eq!(
                (first.delay, first.health, first.spawn_row),
                (first_delay, 32000, first_row),
                "{}",
                data.wad
            );

            // Every run ends with the finish entity's record: delay 400,
            // health 0x14, spawn row 209.
            let last = records.last().expect("populated run");
            assert_eq!(
                (last.delay, last.health, last.spawn_row),
                (400, 20, 209),
                "{}",
                data.wad
            );
        }
    }
}

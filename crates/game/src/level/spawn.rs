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
    /// `table` is the file offset of the constant header slot; the records
    /// follow it. The field mapping onto [`Record`] is provisional: the race
    /// records share the 8-byte shape, but their word semantics are still
    /// open pending render validation (`word0` looks like a BIN reference,
    /// not an x-step).
    StaticTable { table: usize },
}

impl SpawnSource {
    /// Resolves the source into the level's spawn-record buffer.
    ///
    /// `wad` is the level's WAD image (read by [`StaticTable`]
    /// (SpawnSource::StaticTable)); `seed` feeds the engine PRNG (drawn by
    /// [`Generated`](SpawnSource::Generated), pass
    /// [`clock_seed`](super::prng::clock_seed) for the original's
    /// varies-every-play behavior).
    pub fn records(self, wad: &[u8], seed: u16) -> Vec<Record> {
        match self {
            SpawnSource::Generated { script, post_pass } => {
                let post = post_pass.map_or_else(Vec::new, |build| build());
                generate(&script(), &post, &mut EngineRng::new(seed))
            }
            SpawnSource::StaticTable { table } => static_records(wad, table),
        }
    }
}

/// Reads the populated run of a race WAD's static spawn table.
///
/// Slot 0 is the constant header; the populated records follow, padded with
/// `(0, 0, 0, 20)` slots to the table's fixed capacity. A populated record
/// always carries a BIN reference in its first word, so the first zero word0
/// ends the run (each run's last record is a `(ref, 20, 209, 20)` trailer,
/// shared by all three race WADs).
fn static_records(wad: &[u8], table: usize) -> Vec<Record> {
    let mut out = Vec::new();

    for slot in 1..STATIC_SLOTS {
        let bytes = &wad[table + slot * 8..table + (slot + 1) * 8];

        if bytes[0] == 0 && bytes[1] == 0 {
            break;
        }

        let word = |k: usize| u16::from_le_bytes([bytes[k * 2], bytes[k * 2 + 1]]);
        out.push(Record {
            x_step: word(0),
            sprite: word(1),
            depth: word(2),
            y: word(3),
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
        let records = source.records(&[], 0x1a94);

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

        for (level, expected_count, header_word3) in [
            (Level::L2, 67, 200),
            (Level::L4, 82, 200),
            (Level::L6, 242, 100),
        ] {
            let data = level.data();
            let wad = disc.read(data.wad).expect("reading the race WAD");
            let SpawnSource::StaticTable { table } = data.spawns else {
                panic!("race level without a static table");
            };

            // The header slot pins the table offset: three zero words, then a
            // per-level word3 (200 in L2/L4, 100 in L6; meaning unknown).
            assert_eq!(
                &wad[table..table + 8],
                &[0, 0, 0, 0, 0, 0, header_word3, 0],
                "{}: header slot",
                data.wad
            );

            let records = data.spawns.records(&wad, 0);
            assert_eq!(records.len(), expected_count, "{}", data.wad);

            // Every run ends with the shared trailer record.
            let last = records.last().expect("populated run");
            assert_eq!(
                (last.sprite, last.depth, last.y),
                (20, 209, 20),
                "{}",
                data.wad
            );
        }
    }
}

//! `HIGH.TXT`: the high-score table.
//!
//! Eight fixed-width records of 22 bytes each (176 total). A record is a 13-char
//! name, left-aligned and `.`-padded, a space, a 6-digit zero-padded score,
//! then `$\n`:
//!
//! ```text
//! ERIK......... 010000$\n
//! ```
//!
//! The `$` is a leftover DOS string terminator (`int 21h` AH=09) that the
//! original kept in the stored format. The file is ASCII, so this parses and
//! formats through [`FromStr`]/[`Display`] rather than raw byte decoders.
//! Source: the shipped `HIGH.TXT` plus `START.EXE`'s table handling (`0x3f0c`).

use std::fmt::{self, Display};
use std::str::FromStr;

use crate::Result;
use crate::error::DecodeError;

/// Number of entries in the table.
pub const ENTRY_COUNT: usize = 8;
/// Width of the `.`-padded name field.
const NAME_LEN: usize = 13;
/// Width of the zero-padded score field.
const SCORE_LEN: usize = 6;
/// On-disk record length: name, space, score, `$`, newline.
const RECORD_LEN: usize = NAME_LEN + 1 + SCORE_LEN + 1 + 1;

/// One ranked entry: a name and its score.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Highscore {
    pub name: String,
    pub score: u32,
}

/// The eight-entry high-score table, ordered best first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Highscores([Highscore; ENTRY_COUNT]);

impl Highscores {
    /// The entries, best first.
    pub fn entries(&self) -> &[Highscore; ENTRY_COUNT] {
        &self.0
    }

    /// Insert `entry` if its score makes the table, returning whether it did.
    ///
    /// It lands just below every entry whose score is greater than *or equal*
    /// to it, so an older entry keeps the higher rank on a tie, and the bottom
    /// entry falls off. A score that only ties (or trails) the lowest entry
    /// does not make the table. This mirrors `START.EXE`: the qualify test is
    /// strict (`new > lowest`, `0x3f32`) and the insert scan stops at the first
    /// `>=` entry (`0x3ff2`).
    pub fn add(&mut self, entry: Highscore) -> bool {
        let Some(position) = self
            .0
            .iter()
            .position(|existing| existing.score < entry.score)
        else {
            return false;
        };

        self.0[position..].rotate_right(1);
        self.0[position] = entry;
        true
    }
}

impl FromStr for Highscores {
    type Err = DecodeError;

    fn from_str(text: &str) -> Result<Self> {
        let lines: Vec<&str> = text.lines().collect();

        if lines.len() != ENTRY_COUNT {
            return Err(DecodeError::Unrecognized {
                reason: "high-score table needs eight lines",
            });
        }

        let entries = lines
            .iter()
            .map(|line| parse_record(line))
            .collect::<Result<Vec<_>>>()?;

        Ok(Self(
            entries.try_into().expect("collected exactly ENTRY_COUNT"),
        ))
    }
}

impl Display for Highscores {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for highscore in &self.0 {
            writeln!(formatter, "{:.<13} {:06}$", highscore.name, highscore.score)?;
        }

        Ok(())
    }
}

/// Parse one record (a line with its `$`, the newline already stripped).
fn parse_record(line: &str) -> Result<Highscore> {
    if !line.is_ascii() || line.len() != RECORD_LEN - 1 {
        return Err(DecodeError::Unrecognized {
            reason: "high-score record is malformed",
        });
    }

    let (name_field, rest) = line.split_at(NAME_LEN);
    let (separator, rest) = rest.split_at(1);
    let (score_field, terminator) = rest.split_at(SCORE_LEN);

    if separator != " " || terminator != "$" {
        return Err(DecodeError::Unrecognized {
            reason: "high-score record is malformed",
        });
    }

    let score = score_field
        .parse::<u32>()
        .map_err(|_| DecodeError::Unrecognized {
            reason: "high-score is not a number",
        })?;

    Ok(Highscore {
        name: name_field.trim_end_matches('.').to_string(),
        score,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn highscore(name: &str, score: u32) -> Highscore {
        Highscore {
            name: name.to_string(),
            score,
        }
    }

    fn table(scores: [u32; ENTRY_COUNT]) -> Highscores {
        Highscores(scores.map(|score| highscore("X", score)))
    }

    /// Synthetic table (placeholder names/scores, no shipped data) for
    /// exercising the format. Real-file fidelity is checked in the disc-gated
    /// integration golden, which reads HIGH.TXT off the disc.
    fn sample() -> Highscores {
        Highscores([
            highscore("ALICE", 9000),
            highscore("BOB", 8000),
            highscore("CAROL", 7000),
            highscore("DAVE", 6000),
            highscore("EVE", 5000),
            highscore("FRANK", 4000),
            highscore("GRACE", 3000),
            highscore("HEIDI", 2000),
        ])
    }

    #[test]
    fn formats_records_at_the_fixed_width() {
        let text = sample().to_string();

        assert_eq!(text.len(), ENTRY_COUNT * RECORD_LEN);
        assert!(text.lines().all(|line| line.len() == RECORD_LEN - 1));
    }

    #[test]
    fn round_trips_text_to_table_and_back() {
        let scores = sample();
        let text = scores.to_string();

        assert_eq!(text.parse::<Highscores>().unwrap(), scores);
        assert_eq!(scores.entries()[0], highscore("ALICE", 9000));
        assert_eq!(scores.entries()[7], highscore("HEIDI", 2000));
    }

    #[test]
    fn rejects_wrong_line_count() {
        let one_line = "ALICE........ 009000$\n";
        assert!(one_line.parse::<Highscores>().is_err());
    }

    #[test]
    fn rejects_a_non_numeric_score() {
        let broken = sample().to_string().replacen("009000", "0x0000", 1);
        assert!(broken.parse::<Highscores>().is_err());
    }

    #[test]
    fn a_top_score_lands_first_and_drops_the_last() {
        let mut scores = table([80, 70, 60, 50, 40, 30, 20, 10]);

        assert!(scores.add(highscore("NEW", 100)));
        assert_eq!(scores.entries()[0], highscore("NEW", 100));
        assert_eq!(scores.entries()[7], highscore("X", 20)); // the 10 fell off
    }

    #[test]
    fn a_tie_keeps_the_older_entry_higher() {
        let mut scores = table([80, 70, 60, 50, 40, 30, 20, 10]);

        // Ties the 60 entry (rank 3); the newcomer must land just below it.
        assert!(scores.add(highscore("NEW", 60)));
        assert_eq!(scores.entries()[2], highscore("X", 60)); // existing 60 stays
        assert_eq!(scores.entries()[3], highscore("NEW", 60)); // newcomer below it
    }

    #[test]
    fn tying_the_lowest_does_not_make_the_table() {
        let before = table([80, 70, 60, 50, 40, 30, 20, 10]);
        let mut scores = before.clone();

        assert!(!scores.add(highscore("NEW", 10)));
        assert_eq!(scores, before);
    }

    #[test]
    fn trailing_the_lowest_does_not_make_the_table() {
        let before = table([80, 70, 60, 50, 40, 30, 20, 10]);
        let mut scores = before.clone();

        assert!(!scores.add(highscore("NEW", 5)));
        assert_eq!(scores, before);
    }
}

//! The level-layout generator.
//!
//! Prototype's generated levels (1, 3, 5, 7) build their enemy/pickup spawn
//! placement at load from a PRNG-driven layout script rather than storing it.
//! This module reproduces that faithfully: the engine PRNG, the per-level
//! dispatcher script, and the emitter library that writes the spawn records.
//! See `reference/formats/level-layout.md` for the disassembly it mirrors.

pub mod level_1;
pub mod level_3;
pub mod level_5;
pub mod level_7;
pub mod prng;
pub mod slot;
pub mod spawn;

/// A stable fingerprint of a generated layout, for full-buffer golden tests.
///
/// FNV-1a over every record's little-endian `u16` fields. It is deterministic
/// across platforms and Rust versions (unlike `std`'s `DefaultHasher`), so the
/// hex digest can be committed as a golden and will only change if the generated
/// records change. Not cryptographic; it is a regression fence, not a security
/// boundary.
#[cfg(test)]
pub(crate) fn golden_hash(records: &[slot::Record]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;

    for record in records {
        for word in [record.delay, record.sprite, record.health, record.spawn_row] {
            for byte in word.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
    }

    format!("{hash:016x}")
}

//! Enemy-AI helpers that are byte-identical across the per-level AI modules.
//!
//! Only verbatim duplicates live here. The per-level variants (path stepping,
//! the shooter/popper animations) that merely share a name stay in their own
//! module, mirroring the original's separate per-WAD code.

use crate::spawns::Entity;

/// Reads an i16 word from the WAD image, returning 0 past the end.
pub(crate) fn word(wad: &[u8], at: usize) -> i32 {
    if wad.len() < at + 2 {
        return 0;
    }

    i32::from(i16::from_le_bytes([wad[at], wad[at + 1]]))
}

/// Pickup drifters (the levels' funcs 0-3): drift left at 1.25 px, period-4
/// cycle `rest -> rest+0x1e -> +8 steps -> last -> rest`.
pub(crate) fn pickup(entity: &mut Entity, rest: u16, last: u16) {
    entity.x -= 0x14;
    entity.tick += 1;
    entity.anim += 1;

    if entity.anim == 4 {
        entity.anim = 0;
        entity.sprite = if entity.sprite == rest {
            rest + 0x1e
        } else if entity.sprite == last {
            rest
        } else {
            entity.sprite + 8
        };
    }
}

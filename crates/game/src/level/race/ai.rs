//! The race levels' 6 mode-0 AI functions, transcribed from LEVEL_2.WAD.
//!
//! See `reference/race-mode.md` (table file `0xa476`, identical relinked code in
//! LEVEL_4/LEVEL_6).
//!
//! Everything drifts left at a constant 5 px per sub-step. Args 0..3 are the
//! four pickups with the usual rest -> rest+0x1e -> 8-byte cycle animation;
//! their frame runs sit at fixed offsets from the rest descriptor in all
//! three WADs, so the cycle derives from the entity's kind. Arg 4 is every
//! obstacle (no animation); arg 5 is the stationary finish entity, which
//! raises the level-end flag on its first step.

use crate::spawns::Entity;

/// Per-step context for the race AI.
///
/// The AI touches nothing but the entity (passed separately) and the level-end
/// flag carried here.
pub(crate) struct AiContext<'a> {
    /// The level-end flag (`cs:0xcc3`), raised by the finish entity.
    pub level_end: &'a mut bool,
}

/// The pickup frame runs' last-frame offsets from the rest descriptor, per arg.
///
/// Verified identical in all three race WADs.
const PICKUP_LAST_OFFSET: [u16; 4] = [0x5e, 0x5e, 0x6e, 0x46];

/// Runs AI function `arg` for one sub-step.
pub(crate) fn step(entity: &mut Entity, ctx: &mut AiContext) {
    match entity.arg {
        0..=3 => {
            entity.x -= 0x50;
            entity.tick += 1;
            entity.anim += 1;

            if entity.anim == 4 {
                entity.anim = 0;
                let rest = entity.kind;
                let last = rest + PICKUP_LAST_OFFSET[usize::from(entity.arg)];
                entity.sprite = if entity.sprite == rest {
                    rest + 0x1e
                } else if entity.sprite == last {
                    rest
                } else {
                    entity.sprite + 8
                };
            }
        }
        4 => {
            entity.x -= 0x50;
            entity.tick += 1;
        }
        5 => {
            // The finish gate: stationary, ends the race once live.
            entity.tick += 1;
            *ctx.level_end = true;
        }
        _ => {}
    }
}

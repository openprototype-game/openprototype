//! LEVEL_7 (CITY) layout data: the dispatcher script, emitter constants, and the
//! find-by-position insert post-pass, transcribed from the disassembly and
//! validated byte-for-byte against the running game (seed `0x3e94` reproduces the
//! GET-READY capture). See `reference/formats/level-layout.md`.

use super::slot::{Cell, Emitter, Insert, PostOp, Step, XStart, rand, step};

/// The foreground depth the landmark `Once` emitters hardcode (`0xfa`).
const FOREGROUND: u16 = 0xfa;

/// LEVEL_7's only scatter emitter is the L3-shape Grid: `outer = rng(dx) + cx`
/// rows of `inner = rng(ax) + bx` records, the per-row y `rng(0xa)` plus the
/// row-y offset slot. sprite/depth/x-step/row-reset come from the slots.
fn grid(ax: u16, bx: u16, cx: u16, dx: u16) -> Emitter {
    Emitter::Grid {
        outer: rand(dx, cx),
        inner: rand(ax, bx),
        row_y: rand(0xa, 0),
        row_y_uses_offset: true,
    }
}

// Landmark `Once` emitters: one record, x = x_start, foreground depth.

fn once_41b5() -> Emitter {
    Emitter::Once {
        sprite: 0x41b5,
        depth: FOREGROUND,
        y: rand(3, 0),
    }
}

fn once_421b() -> Emitter {
    Emitter::Once {
        sprite: 0x421b,
        depth: FOREGROUND,
        y: rand(3, 3),
    }
}

fn once_4357() -> Emitter {
    Emitter::Once {
        sprite: 0x4357,
        depth: FOREGROUND,
        y: rand(2, 6),
    }
}

// Fixed `0x4959` landmark blocks (no PRNG). The lead x is x_start alone (no step).

fn lead_4959(y: u16) -> Cell {
    Cell {
        x_base: 0,
        x_start: XStart::Consume,
        sprite: 0x4959,
        depth: 0xaf0,
        y,
    }
}

fn tail_4959(x_base: u16, y: u16) -> Cell {
    Cell {
        x_base,
        x_start: XStart::None,
        sprite: 0x4959,
        depth: 0xaf0,
        y,
    }
}

fn fixed_4959_1() -> Emitter {
    Emitter::Fixed {
        repeat: None,
        cells: vec![lead_4959(0x7d)],
    }
}

fn fixed_4959_2() -> Emitter {
    Emitter::Fixed {
        repeat: None,
        cells: vec![lead_4959(0x7b), tail_4959(0x28, 0x7e)],
    }
}

fn fixed_4959_3() -> Emitter {
    Emitter::Fixed {
        repeat: None,
        cells: vec![lead_4959(0x7d), tail_4959(0, 0x81), tail_4959(0, 0x83)],
    }
}

/// LEVEL_7's 28-step append script, in order (21 Grid calls + 7 landmarks). A
/// step omits `x_start` when the original leaves it carried from the previous
/// emitter (a Grid leaves it at its row-reset, which the next landmark reads).
pub fn script() -> Vec<Step> {
    vec![
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x45d1)
            .depth(0xdc)
            .row_reset(0x32)
            .row_y_offset(0x67)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x4a2f)
            .depth(0xaa)
            .row_reset(0x32)
            .row_y_offset(0x3f)
            .emit(grid(0x0, 0x1, 0xf, 0x5)),
        step()
            .x_start(0x14)
            .x_step(0x0)
            .sprite(0x4559)
            .depth(0xdc)
            .row_reset(0x32)
            .row_y_offset(0x53)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x45d1)
            .depth(0xdc)
            .row_reset(0x32)
            .row_y_offset(0x71)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
        step()
            .x_start(0x64)
            .x_step(0x14)
            .sprite(0x4a2f)
            .depth(0xaa)
            .row_reset(0x96)
            .row_y_offset(0x3f)
            .emit(grid(0x5, 0x6, 0x5, 0x5)),
        step().emit(once_4357()),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x45d1)
            .depth(0xdc)
            .row_reset(0x32)
            .row_y_offset(0x67)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
        step().x_start(0x64).emit(fixed_4959_1()),
        step()
            .x_start(0x64)
            .x_step(0x14)
            .sprite(0x4a2f)
            .depth(0xaa)
            .row_reset(0x96)
            .row_y_offset(0x3f)
            .emit(grid(0x5, 0x6, 0x5, 0x5)),
        step().x_start(0x2).emit(once_421b()),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x4559)
            .depth(0xdc)
            .row_reset(0x32)
            .row_y_offset(0x5d)
            .emit(grid(0x0, 0x1, 0x5, 0x5)),
        step()
            .x_start(0x64)
            .x_step(0x14)
            .sprite(0x4aa7)
            .depth(0x96)
            .row_reset(0x96)
            .row_y_offset(0x35)
            .emit(grid(0x5, 0x6, 0x5, 0x5)),
        step().x_start(0xc8).emit(fixed_4959_2()),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x4b97)
            .depth(0x96)
            .row_reset(0x32)
            .row_y_offset(0x17)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
        step().x_start(0x2).emit(once_41b5()),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x45d1)
            .depth(0xdc)
            .row_reset(0x32)
            .row_y_offset(0x67)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
        step().x_start(0xc8).emit(fixed_4959_3()),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x4c87)
            .depth(0xdc)
            .row_reset(0x32)
            .row_y_offset(0xd)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x4aa7)
            .depth(0x96)
            .row_reset(0x32)
            .row_y_offset(0x21)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x4a2f)
            .depth(0xaa)
            .row_reset(0x32)
            .row_y_offset(0x49)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
        step().x_start(0x2).emit(once_41b5()),
        step()
            .x_start(0x64)
            .x_step(0x14)
            .sprite(0x4aa7)
            .depth(0x96)
            .row_reset(0x96)
            .row_y_offset(0x35)
            .emit(grid(0x5, 0x6, 0x5, 0x5)),
        step()
            .x_start(0x64)
            .x_step(0x14)
            .sprite(0x4b97)
            .depth(0x96)
            .row_reset(0x96)
            .row_y_offset(0x2b)
            .emit(grid(0x5, 0x6, 0x5, 0x5)),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x4c87)
            .depth(0xdc)
            .row_reset(0x32)
            .row_y_offset(0xd)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x4aa7)
            .depth(0x96)
            .row_reset(0x32)
            .row_y_offset(0x35)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x4a2f)
            .depth(0xaa)
            .row_reset(0x32)
            .row_y_offset(0x3f)
            .emit(grid(0x0, 0x1, 0xf, 0x5)),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x4b97)
            .depth(0x96)
            .row_reset(0x32)
            .row_y_offset(0x2b)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
        step()
            .x_start(0x64)
            .x_step(0x0)
            .sprite(0x4c87)
            .depth(0xdc)
            .row_reset(0x32)
            .row_y_offset(0xd)
            .emit(grid(0x0, 0x1, 0xa, 0x5)),
    ]
}

fn landmark(target_x: u16, y: u16) -> PostOp {
    PostOp::Insert(Insert {
        target_x,
        records: vec![(0x4689, 0x140, y)],
    })
}

/// LEVEL_7's find-by-position insert post-pass: a 5-record `0x7d00` template
/// block, then eight `0x4689` landmarks, each inserted at its absolute-x.
pub fn post_pass() -> Vec<PostOp> {
    vec![
        PostOp::Insert(Insert {
            target_x: 0x3cd2,
            records: vec![
                (0x5893, 0x7d00, 0x8),
                (0x4cbd, 0x7d00, 0x9),
                (0x507d, 0x7d00, 0xa),
                (0x53a7, 0x7d00, 0xb),
                (0x5749, 0x7d00, 0xc),
            ],
        }),
        landmark(0xb30, 0x87),
        landmark(0x161c, 0x85),
        landmark(0x16a0, 0x85),
        landmark(0x2047, 0x85),
        landmark(0x2097, 0x85),
        landmark(0x260a, 0x87),
        landmark(0x2e21, 0x85),
        landmark(0x2e7f, 0x85),
    ]
}

#[cfg(test)]
mod tests {
    use super::{post_pass, script};
    use crate::level::golden_hash;
    use crate::level::prng::EngineRng;
    use crate::level::slot::generate;

    /// FNV-1a over the full 496-record buffer (post-pass included) for the
    /// validated seed. Locks the layout byte-for-byte against refactors;
    /// regenerate and re-verify against the capture if it ever changes.
    const GOLDEN: &str = "f39ee1d0bdd4522e";

    #[test]
    fn reproduces_the_validated_capture() {
        // Seed 0x3e94 is the wall-clock seed of the captured GET-READY state.
        let records = generate(&script(), &post_pass(), &mut EngineRng::new(0x3e94));

        assert_eq!(records.len(), 496);
        assert_eq!(golden_hash(&records), GOLDEN);
    }

    #[test]
    fn is_deterministic_for_a_seed() {
        let a = generate(&script(), &post_pass(), &mut EngineRng::new(0x1234));
        let b = generate(&script(), &post_pass(), &mut EngineRng::new(0x1234));
        assert_eq!(a, b);
    }
}

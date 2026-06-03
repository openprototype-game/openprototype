//! LEVEL_5 (TECHNO) layout data: the dispatcher script and emitter constants,
//! transcribed from the disassembly and validated byte-for-byte against the
//! running game (seed `0x2d93` reproduces the GET-READY capture). See
//! `reference/formats/level-layout.md`.

use super::generator::rand;
use super::slot::{Emitter, Step, step};

// Per-sprite-type depth (parallax layer), read by the original from runtime
// slots `cs:[bd9e..bdae]`. Two sprites share `D_3C4E`.
const D_3A2C_A: u16 = 0xfa;
const D_3A2C_B: u16 = 0x96;
const D_3AC2: u16 = 0x708;
const D_3B70: u16 = 0x10e7;
const D_3C4E: u16 = 0x32;
const D_3CF0: u16 = 0x96;
const D_3D46_A: u16 = 0x140;
const D_3D46_B: u16 = 0x5aa;
const D_426E: u16 = 0x3a98;

/// The foreground depth the landmark `Once` emitters hardcode (`0xfa`).
const FOREGROUND: u16 = 0xfa;

// Landmark emitters: one record, x = x_start (no step), foreground depth.

fn once_3688() -> Emitter {
    Emitter::Once {
        sprite: 0x3688,
        depth: FOREGROUND,
        y: rand(3, 0),
    }
}

fn once_36ee() -> Emitter {
    Emitter::Once {
        sprite: 0x36ee,
        depth: FOREGROUND,
        y: rand(3, 3),
    }
}

fn once_382a() -> Emitter {
    Emitter::Once {
        sprite: 0x382a,
        depth: FOREGROUND,
        y: rand(3, 6),
    }
}

// Looping emitters: `count = rng(ax) + bx`, per-record y. Named for the sprite
// they place and, where two routines share a sprite, their y band.

fn single_3a2c_a(ax: u16, bx: u16) -> Emitter {
    Emitter::Single {
        count: rand(ax, bx),
        sprite: 0x3a2c,
        depth: D_3A2C_A,
        y: rand(9, 0xc),
    }
}

fn single_3a2c_b(ax: u16, bx: u16) -> Emitter {
    Emitter::Single {
        count: rand(ax, bx),
        sprite: 0x3a2c,
        depth: D_3A2C_B,
        y: rand(6, 0x54),
    }
}

fn single_3ac2(ax: u16, bx: u16) -> Emitter {
    Emitter::Single {
        count: rand(ax, bx),
        sprite: 0x3ac2,
        depth: D_3AC2,
        y: rand(3, 0x5a),
    }
}

fn single_3c84(ax: u16, bx: u16) -> Emitter {
    Emitter::Single {
        count: rand(ax, bx),
        sprite: 0x3c84,
        depth: D_3C4E,
        y: rand(0xb, 0x1e),
    }
}

fn single_3c4e_2(ax: u16, bx: u16) -> Emitter {
    Emitter::Single {
        count: rand(ax, bx),
        sprite: 0x3c4e,
        depth: D_3C4E,
        y: rand(0xe, 0x29),
    }
}

fn single_3c84_2(ax: u16, bx: u16) -> Emitter {
    Emitter::Single {
        count: rand(ax, bx),
        sprite: 0x3c84,
        depth: D_3C4E,
        y: rand(0xe, 0x37),
    }
}

fn single_3d46_a(ax: u16, bx: u16) -> Emitter {
    Emitter::Single {
        count: rand(ax, bx),
        sprite: 0x3d46,
        depth: D_3D46_A,
        y: rand(7, 0x49),
    }
}

fn single_3d46_b(ax: u16, bx: u16) -> Emitter {
    Emitter::Single {
        count: rand(ax, bx),
        sprite: 0x3d46,
        depth: D_3D46_B,
        y: rand(3, 0x50),
    }
}

// Row emitters: `count = rng(ax) + bx` records sharing one y, drawn once.

fn row_3a2c(ax: u16, bx: u16) -> Emitter {
    Emitter::Row {
        count: rand(ax, bx),
        sprite: 0x3a2c,
        depth: D_3A2C_A,
        y: rand(9, 0xc),
    }
}

fn row_3c84(ax: u16, bx: u16) -> Emitter {
    Emitter::Row {
        count: rand(ax, bx),
        sprite: 0x3c84,
        depth: D_3C4E,
        y: rand(0xb, 0x1e),
    }
}

/// `0xffe0`: `rows = rng(ax) + bx` rows; one rng(3) picks the y-pair for all of
/// them. Each row emits two `0x3cf0` records (lead x = x_start + x_step, tail
/// x = 0).
fn grid_3cf0(ax: u16, bx: u16) -> Emitter {
    Emitter::BranchRows {
        rows: rand(ax, bx),
        sprite: 0x3cf0,
        depth: D_3CF0,
        if_one: (0x47, 0x48),
        otherwise: (0x45, 0x46),
    }
}

/// `0x10119`: one `0x3b70` record, x = x_start + x_step, fixed y, no draws.
fn fixed_3b70() -> Emitter {
    Emitter::Fixed {
        lead_sprite: 0x3b70,
        lead_depth: D_3B70,
        lead_y: 0x53,
        rest: Vec::new(),
    }
}

/// `0x10146`: the post-amble — a `0x426e` landmark (x = x_start + x_step) then
/// five fixed `0x3764` background records with stepping y.
fn tail_426e() -> Emitter {
    Emitter::Fixed {
        lead_sprite: 0x426e,
        lead_depth: D_426E,
        lead_y: 0x5d,
        rest: vec![
            (0x96, 0x3764, FOREGROUND, 0x5e),
            (0, 0x3764, FOREGROUND, 0x5f),
            (0, 0x3764, FOREGROUND, 0x60),
            (0, 0x3764, FOREGROUND, 0x61),
            (0, 0x3764, FOREGROUND, 0x62),
        ],
    }
}

/// LEVEL_5's 48-step append script, in order. Steps set only the slots the
/// original writes; the rest carry over. Three steps repeat under a
/// dispatcher-level loop.
pub fn script() -> Vec<Step> {
    vec![
        step()
            .x_start(0x96)
            .x_step(0x1e)
            .emit(single_3c4e_2(5, 0xf)),
        step().x_step(0x14).emit(single_3c4e_2(0xa, 0x14)),
        step().x_step(0xf).emit(single_3c4e_2(0xa, 0xa)),
        step().x_step(0xa).emit(single_3c4e_2(0xa, 0xa)),
        step().x_start(0).x_step(0x168).emit(single_3d46_b(6, 5)),
        step().x_start(5).emit(once_3688()),
        step().x_step(0xa).emit(row_3c84(5, 0xa)),
        step().x_start(0x78).emit(row_3c84(5, 0xa)),
        step().x_start(5).emit(once_3688()),
        step()
            .x_start(0x78)
            .x_step(0x14)
            .repeat(rand(3, 2))
            .emit(grid_3cf0(5, 0xa)),
        step()
            .x_start(0x64)
            .x_step(0x14)
            .emit(single_3a2c_b(0xa, 0xa)),
        step().x_start(5).emit(once_382a()),
        step()
            .x_start(0x64)
            .x_step(0x1e)
            .emit(single_3c84(0xa, 0xa)),
        step().x_start(5).emit(once_3688()),
        step().x_start(0x78).x_step(0x82).emit(single_3d46_a(5, 5)),
        step().x_start(0x96).x_step(0x14).emit(single_3c4e_2(5, 5)),
        step().x_step(0x14).emit(single_3c84_2(5, 5)),
        step().x_step(0x14).emit(single_3c4e_2(5, 5)),
        step().x_step(0x14).emit(single_3c84_2(5, 5)),
        step().x_start(5).emit(once_3688()),
        step()
            .x_start(0x64)
            .x_step(0x14)
            .repeat(rand(3, 2))
            .emit(row_3a2c(0xa, 0xa)),
        step().x_start(0).x_step(0x168).emit(single_3d46_b(2, 2)),
        step().x_start(0x64).x_step(0).emit(fixed_3b70()),
        step()
            .x_start(0x64)
            .x_step(0x14)
            .emit(single_3c84_2(0xa, 0x14)),
        step().x_start(5).emit(once_3688()),
        step().x_start(0x64).x_step(0xdc).emit(single_3ac2(1, 2)),
        step().x_start(0x78).x_step(0x14).emit(grid_3cf0(5, 0xa)),
        step().x_start(0x78).x_step(0x14).emit(grid_3cf0(5, 0xa)),
        step().x_start(0x64).x_step(0).emit(single_3ac2(1, 1)),
        step().x_start(5).emit(once_3688()),
        step().x_start(1).x_step(0).emit(fixed_3b70()),
        step()
            .x_start(0x78)
            .x_step(0x46)
            .emit(single_3d46_a(0xa, 0xa)),
        step().x_start(0x64).x_step(0x14).emit(single_3c84_2(5, 5)),
        step().x_step(0x14).emit(single_3c4e_2(5, 5)),
        step().x_start(5).emit(once_36ee()),
        step().x_start(0x64).x_step(0).emit(single_3ac2(1, 1)),
        step().x_start(1).x_step(0).emit(single_3d46_b(1, 1)),
        step().x_start(1).x_step(0).emit(fixed_3b70()),
        step()
            .x_start(0x78)
            .x_step(0xa)
            .repeat(rand(3, 4))
            .emit(row_3c84(7, 8)),
        step()
            .x_start(0x64)
            .x_step(0x1e)
            .emit(single_3a2c_a(5, 0xa)),
        step().x_start(5).emit(once_382a()),
        step().x_start(0x14).x_step(0).emit(single_3ac2(1, 1)),
        step()
            .x_start(0x3c)
            .x_step(0x14)
            .emit(single_3c4e_2(0xb, 0xa)),
        step().x_start(5).emit(once_3688()),
        step().x_start(0).x_step(0x168).emit(single_3d46_b(1, 3)),
        step().x_start(0x78).x_step(0x14).emit(grid_3cf0(0xb, 0xa)),
        step().x_start(0x78).x_step(0x14).emit(grid_3cf0(0xb, 0xa)),
        step().emit(tail_426e()),
    ]
}

#[cfg(test)]
mod tests {
    use super::script;
    use crate::level::golden_hash;
    use crate::level::prng::EngineRng;
    use crate::level::slot::generate;

    /// FNV-1a over the full 521-record buffer for the validated seed. Locks the
    /// layout byte-for-byte against refactors; regenerate and re-verify against
    /// the capture if it ever changes.
    const GOLDEN: &str = "148dc8cb0a3a0fe6";

    #[test]
    fn reproduces_the_validated_capture() {
        // Seed 0x2d93 is the wall-clock seed of the captured GET-READY state.
        let records = generate(&script(), &[], &mut EngineRng::new(0x2d93));

        assert_eq!(records.len(), 521);
        assert_eq!(golden_hash(&records), GOLDEN);
    }

    #[test]
    fn is_deterministic_for_a_seed() {
        let a = generate(&script(), &[], &mut EngineRng::new(0x1234));
        let b = generate(&script(), &[], &mut EngineRng::new(0x1234));
        assert_eq!(a, b);
    }
}

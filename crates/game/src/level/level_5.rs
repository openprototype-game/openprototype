//! LEVEL_5 (TECHNO) layout data: the dispatcher script and emitter constants.
//!
//! Transcribed from the disassembly and validated byte-for-byte against the
//! running game (seed `0x2d93` reproduces the GET-READY capture). See
//! `reference/formats/level-layout.md`.

use super::slot::{Cell, Emitter, Fill, RowStyle, Step, XStart, rand, step};

// Per-sprite-type spawn health, read by the original from runtime slots
// `cs:[bd9e..bdae]`. Two sprites share `D_3C4E`.
const D_3A2C_A: u16 = 0xfa;
const D_3A2C_B: u16 = 0x96;
const D_3AC2: u16 = 0x708;
const D_3B70: u16 = 0x10e7;
const D_3C4E: u16 = 0x32;
const D_3CF0: u16 = 0x96;
const D_3D46_A: u16 = 0x140;
const D_3D46_B: u16 = 0x5aa;
const D_426E: u16 = 0x3a98;

/// The health the landmark pickup `Once` emitters hardcode (`0xfa` = 250).
const PICKUP_HEALTH: u16 = 0xfa;

// Landmark emitters: one record, delay = x_start (no step), pickup health.

fn once_3688() -> Emitter {
    Emitter::Once {
        sprite: 0x3688,
        health: PICKUP_HEALTH,
        spawn_row: rand(3, 0),
    }
}

fn once_36ee() -> Emitter {
    Emitter::Once {
        sprite: 0x36ee,
        health: PICKUP_HEALTH,
        spawn_row: rand(3, 3),
    }
}

fn once_382a() -> Emitter {
    Emitter::Once {
        sprite: 0x382a,
        health: PICKUP_HEALTH,
        spawn_row: rand(3, 6),
    }
}

// Looping emitters: `count = rng(ax) + bx`, per-record spawn row. Named for
// the sprite they place and, where two routines share a sprite, their row
// band.

fn single_3a2c_a(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(9, 0xc),
        fill: Fill::Baked {
            sprite: 0x3a2c,
            health: D_3A2C_A,
        },
    }
}

fn single_3a2c_b(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(6, 0x54),
        fill: Fill::Baked {
            sprite: 0x3a2c,
            health: D_3A2C_B,
        },
    }
}

fn single_3ac2(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(3, 0x5a),
        fill: Fill::Baked {
            sprite: 0x3ac2,
            health: D_3AC2,
        },
    }
}

fn single_3c84(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(0xb, 0x1e),
        fill: Fill::Baked {
            sprite: 0x3c84,
            health: D_3C4E,
        },
    }
}

fn single_3c4e_2(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(0xe, 0x29),
        fill: Fill::Baked {
            sprite: 0x3c4e,
            health: D_3C4E,
        },
    }
}

fn single_3c84_2(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(0xe, 0x37),
        fill: Fill::Baked {
            sprite: 0x3c84,
            health: D_3C4E,
        },
    }
}

fn single_3d46_a(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(7, 0x49),
        fill: Fill::Baked {
            sprite: 0x3d46,
            health: D_3D46_A,
        },
    }
}

fn single_3d46_b(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(3, 0x50),
        fill: Fill::Baked {
            sprite: 0x3d46,
            health: D_3D46_B,
        },
    }
}

// Row emitters: `count = rng(ax) + bx` records sharing one spawn row, drawn
// once.

fn row_3a2c(ax: u16, bx: u16) -> Emitter {
    Emitter::Row {
        count: rand(ax, bx),
        sprite: 0x3a2c,
        health: D_3A2C_A,
        spawn_row: rand(9, 0xc),
        style: RowStyle::Stepped,
    }
}

fn row_3c84(ax: u16, bx: u16) -> Emitter {
    Emitter::Row {
        count: rand(ax, bx),
        sprite: 0x3c84,
        health: D_3C4E,
        spawn_row: rand(0xb, 0x1e),
        style: RowStyle::Stepped,
    }
}

/// Builds LEVEL_5's `0x3cf0` paired-rows grid for the given counts.
///
/// `rows = rng(ax) + bx` rows; one rng(3) picks the spawn-row pair for all of
/// them.
fn grid_3cf0(ax: u16, bx: u16) -> Emitter {
    Emitter::PairedRows {
        rows: rand(ax, bx),
        sprite: 0x3cf0,
        health: D_3CF0,
        pair_when_one: (0x47, 0x48),
        pair_otherwise: (0x45, 0x46),
    }
}

/// Builds the lone `0x3b70` marker: one stepped record, no draws.
///
/// (orig `0x10119`.)
fn fixed_3b70() -> Emitter {
    Emitter::Fixed {
        repeat: None,
        cells: vec![Cell {
            x_base: 0,
            x_start: XStart::Step,
            sprite: 0x3b70,
            health: D_3B70,
            spawn_row: 0x53,
        }],
    }
}

/// Builds LEVEL_5's post-amble block.
///
/// A `0x426e` landmark (delay = x_start + x_step) then five fixed `0x3764`
/// records with stepping spawn rows. (orig `0x10146`.)
fn tail_426e() -> Emitter {
    let bg = |x_base: u16, spawn_row: u16| Cell {
        x_base,
        x_start: XStart::None,
        sprite: 0x3764,
        health: PICKUP_HEALTH,
        spawn_row,
    };

    Emitter::Fixed {
        repeat: None,
        cells: vec![
            Cell {
                x_base: 0,
                x_start: XStart::Step,
                sprite: 0x426e,
                health: D_426E,
                spawn_row: 0x5d,
            },
            bg(0x96, 0x5e),
            bg(0, 0x5f),
            bg(0, 0x60),
            bg(0, 0x61),
            bg(0, 0x62),
        ],
    }
}

/// Returns LEVEL_5's 48-step append script, in order.
///
/// Steps set only the slots the original writes; the rest carry over. Three
/// steps repeat under a dispatcher-level loop.
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

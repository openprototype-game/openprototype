//! LEVEL_5 (TECHNO) layout data: the dispatcher script and emitter constants.
//!
//! Transcribed from the disassembly and validated byte-for-byte against the
//! running game (seed `0x2d93` reproduces the GET-READY capture). See
//! `reference/formats/level-layout.md`.

use super::{
    BOSS, DESTROYER, DRONE_L, DRONE_R, EXTRA_LIFE, FIGHTER, GUNSHIP, INVINCIBILITY, RAIDER,
    SMART_BOMB, TANK, WEAPON_UPGRADE,
};
use crate::level::slot::{Cell, Emitter, Fill, RowStyle, Step, XStart, rand, step};

// Per-enemy spawn health, read by the original from runtime slots
// `cs:[bd9e..bdae]`. Both DRONEs share DRONE_HEALTH; RAIDER and DESTROYER
// each carry two values.
const RAIDER_HEALTH: u16 = 0xfa;
const RAIDER_HEALTH_B: u16 = 0x96;
const GUNSHIP_HEALTH: u16 = 0x708;
const TANK_HEALTH: u16 = 0x10e7;
const DRONE_HEALTH: u16 = 0x32;
const FIGHTER_HEALTH: u16 = 0x96;
const DESTROYER_HEALTH_A: u16 = 0x140;
const DESTROYER_HEALTH_B: u16 = 0x5aa;
const BOSS_HEALTH: u16 = 0x3a98;

/// The health the landmark pickup `Once` emitters hardcode (`0xfa` = 250).
const PICKUP_HEALTH: u16 = 0xfa;

// Landmark emitters: one record, delay = x_start (no step), pickup health.

fn smart_bomb_once() -> Emitter {
    Emitter::Once {
        sprite: SMART_BOMB,
        health: PICKUP_HEALTH,
        spawn_row: rand(3, 0),
    }
}

fn invincibility_once() -> Emitter {
    Emitter::Once {
        sprite: INVINCIBILITY,
        health: PICKUP_HEALTH,
        spawn_row: rand(3, 3),
    }
}

fn extra_life_once() -> Emitter {
    Emitter::Once {
        sprite: EXTRA_LIFE,
        health: PICKUP_HEALTH,
        spawn_row: rand(3, 6),
    }
}

// Looping emitters: `count = rng(ax) + bx`, per-record spawn row. Named for
// the enemy they place; the _a/_b suffixes are the same enemy at a different
// row band.

fn raider_steps_a(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(9, 0xc),
        fill: Fill::Baked {
            sprite: RAIDER,
            health: RAIDER_HEALTH,
        },
    }
}

fn raider_steps_b(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(6, 0x54),
        fill: Fill::Baked {
            sprite: RAIDER,
            health: RAIDER_HEALTH_B,
        },
    }
}

fn gunship_steps(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(3, 0x5a),
        fill: Fill::Baked {
            sprite: GUNSHIP,
            health: GUNSHIP_HEALTH,
        },
    }
}

fn drone_r_steps(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(0xb, 0x1e),
        fill: Fill::Baked {
            sprite: DRONE_R,
            health: DRONE_HEALTH,
        },
    }
}

fn drone_l_steps(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(0xe, 0x29),
        fill: Fill::Baked {
            sprite: DRONE_L,
            health: DRONE_HEALTH,
        },
    }
}

fn drone_r_steps_2(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(0xe, 0x37),
        fill: Fill::Baked {
            sprite: DRONE_R,
            health: DRONE_HEALTH,
        },
    }
}

fn destroyer_steps_a(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(7, 0x49),
        fill: Fill::Baked {
            sprite: DESTROYER,
            health: DESTROYER_HEALTH_A,
        },
    }
}

fn destroyer_steps_b(ax: u16, bx: u16) -> Emitter {
    Emitter::Steps {
        count: rand(ax, bx),
        spawn_row: rand(3, 0x50),
        fill: Fill::Baked {
            sprite: DESTROYER,
            health: DESTROYER_HEALTH_B,
        },
    }
}

// Row emitters: `count = rng(ax) + bx` records sharing one spawn row, drawn
// once.

fn raider_row(ax: u16, bx: u16) -> Emitter {
    Emitter::Row {
        count: rand(ax, bx),
        sprite: RAIDER,
        health: RAIDER_HEALTH,
        spawn_row: rand(9, 0xc),
        style: RowStyle::Stepped,
    }
}

fn drone_r_row(ax: u16, bx: u16) -> Emitter {
    Emitter::Row {
        count: rand(ax, bx),
        sprite: DRONE_R,
        health: DRONE_HEALTH,
        spawn_row: rand(0xb, 0x1e),
        style: RowStyle::Stepped,
    }
}

/// Builds LEVEL_5's `0x3cf0` paired-rows grid for the given counts.
///
/// `rows = rng(ax) + bx` rows; one rng(3) picks the spawn-row pair for all of
/// them.
fn fighter_grid(ax: u16, bx: u16) -> Emitter {
    Emitter::PairedRows {
        rows: rand(ax, bx),
        sprite: FIGHTER,
        health: FIGHTER_HEALTH,
        pair_when_one: (0x47, 0x48),
        pair_otherwise: (0x45, 0x46),
    }
}

/// Builds the lone `0x3b70` marker: one stepped record, no draws.
///
/// (orig `0x10119`.)
fn tank_marker() -> Emitter {
    Emitter::Fixed {
        repeat: None,
        cells: vec![Cell {
            x_base: 0,
            x_start: XStart::Step,
            sprite: TANK,
            health: TANK_HEALTH,
            spawn_row: 0x53,
        }],
    }
}

/// Builds LEVEL_5's post-amble block.
///
/// A `0x426e` landmark (delay = x_start + x_step) then five fixed `0x3764`
/// records with stepping spawn rows. (orig `0x10146`.)
fn boss_tail() -> Emitter {
    let bg = |x_base: u16, spawn_row: u16| Cell {
        x_base,
        x_start: XStart::None,
        sprite: WEAPON_UPGRADE,
        health: PICKUP_HEALTH,
        spawn_row,
    };

    Emitter::Fixed {
        repeat: None,
        cells: vec![
            Cell {
                x_base: 0,
                x_start: XStart::Step,
                sprite: BOSS,
                health: BOSS_HEALTH,
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
            .emit(drone_l_steps(5, 0xf)),
        step().x_step(0x14).emit(drone_l_steps(0xa, 0x14)),
        step().x_step(0xf).emit(drone_l_steps(0xa, 0xa)),
        step().x_step(0xa).emit(drone_l_steps(0xa, 0xa)),
        step()
            .x_start(0)
            .x_step(0x168)
            .emit(destroyer_steps_b(6, 5)),
        step().x_start(5).emit(smart_bomb_once()),
        step().x_step(0xa).emit(drone_r_row(5, 0xa)),
        step().x_start(0x78).emit(drone_r_row(5, 0xa)),
        step().x_start(5).emit(smart_bomb_once()),
        step()
            .x_start(0x78)
            .x_step(0x14)
            .repeat(rand(3, 2))
            .emit(fighter_grid(5, 0xa)),
        step()
            .x_start(0x64)
            .x_step(0x14)
            .emit(raider_steps_b(0xa, 0xa)),
        step().x_start(5).emit(extra_life_once()),
        step()
            .x_start(0x64)
            .x_step(0x1e)
            .emit(drone_r_steps(0xa, 0xa)),
        step().x_start(5).emit(smart_bomb_once()),
        step()
            .x_start(0x78)
            .x_step(0x82)
            .emit(destroyer_steps_a(5, 5)),
        step().x_start(0x96).x_step(0x14).emit(drone_l_steps(5, 5)),
        step().x_step(0x14).emit(drone_r_steps_2(5, 5)),
        step().x_step(0x14).emit(drone_l_steps(5, 5)),
        step().x_step(0x14).emit(drone_r_steps_2(5, 5)),
        step().x_start(5).emit(smart_bomb_once()),
        step()
            .x_start(0x64)
            .x_step(0x14)
            .repeat(rand(3, 2))
            .emit(raider_row(0xa, 0xa)),
        step()
            .x_start(0)
            .x_step(0x168)
            .emit(destroyer_steps_b(2, 2)),
        step().x_start(0x64).x_step(0).emit(tank_marker()),
        step()
            .x_start(0x64)
            .x_step(0x14)
            .emit(drone_r_steps_2(0xa, 0x14)),
        step().x_start(5).emit(smart_bomb_once()),
        step().x_start(0x64).x_step(0xdc).emit(gunship_steps(1, 2)),
        step().x_start(0x78).x_step(0x14).emit(fighter_grid(5, 0xa)),
        step().x_start(0x78).x_step(0x14).emit(fighter_grid(5, 0xa)),
        step().x_start(0x64).x_step(0).emit(gunship_steps(1, 1)),
        step().x_start(5).emit(smart_bomb_once()),
        step().x_start(1).x_step(0).emit(tank_marker()),
        step()
            .x_start(0x78)
            .x_step(0x46)
            .emit(destroyer_steps_a(0xa, 0xa)),
        step()
            .x_start(0x64)
            .x_step(0x14)
            .emit(drone_r_steps_2(5, 5)),
        step().x_step(0x14).emit(drone_l_steps(5, 5)),
        step().x_start(5).emit(invincibility_once()),
        step().x_start(0x64).x_step(0).emit(gunship_steps(1, 1)),
        step().x_start(1).x_step(0).emit(destroyer_steps_b(1, 1)),
        step().x_start(1).x_step(0).emit(tank_marker()),
        step()
            .x_start(0x78)
            .x_step(0xa)
            .repeat(rand(3, 4))
            .emit(drone_r_row(7, 8)),
        step()
            .x_start(0x64)
            .x_step(0x1e)
            .emit(raider_steps_a(5, 0xa)),
        step().x_start(5).emit(extra_life_once()),
        step().x_start(0x14).x_step(0).emit(gunship_steps(1, 1)),
        step()
            .x_start(0x3c)
            .x_step(0x14)
            .emit(drone_l_steps(0xb, 0xa)),
        step().x_start(5).emit(smart_bomb_once()),
        step()
            .x_start(0)
            .x_step(0x168)
            .emit(destroyer_steps_b(1, 3)),
        step()
            .x_start(0x78)
            .x_step(0x14)
            .emit(fighter_grid(0xb, 0xa)),
        step()
            .x_start(0x78)
            .x_step(0x14)
            .emit(fighter_grid(0xb, 0xa)),
        step().emit(boss_tail()),
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

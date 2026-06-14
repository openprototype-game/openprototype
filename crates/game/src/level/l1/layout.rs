//! LEVEL_1 (CANYON) layout data: script, emitter constants, and health table.
//!
//! Transcribed from the disassembly and validated byte-for-byte against the
//! running game (seed `0x3b95` reproduces the GET-READY capture). See
//! `reference/formats/level-layout.md`.

use super::{
    ASTEROID, BOSS, CANNON, EXTRA_LIFE, INTERCEPTOR, INVINCIBILITY, KAMIKAZE, ORBITER, SMART_BOMB,
    SNIPER, STRAFER, WEAPON_UPGRADE,
};
use crate::level::slot::{Arm, Cell, Emitter, Extra, Rand, RowStyle, Step, XStart, rand, step};

// Per-enemy spawn health, read by the original from a 9-entry table at
// `cs:[bf6d..]`. The boss carries two values (its two end-of-level spawns).

const ASTEROID_HEALTH: u16 = 100;
const KAMIKAZE_HEALTH: u16 = 160;
const CANNON_HEALTH: u16 = 500;
const STRAFER_HEALTH: u16 = 600;
const INTERCEPTOR_HEALTH: u16 = 200;
const SNIPER_HEALTH: u16 = 200;
const ORBITER_HEALTH: u16 = 1000;
const BOSS_HEALTH: u16 = 14000;
const BOSS_HEALTH_2: u16 = 10000;

/// The health the landmark pickup emitters hardcode (`0xfa` = 250).
const PICKUP_HEALTH: u16 = 250;

// Fixed/repeat record blocks (the emitters with no count loop or a constant body).
// Original code addresses: ORBITER_PAIR eb35, BOSS_CELL eb72,
// BOSS_AND_UPGRADES eb92, SNIPER_CELLS ecbd.

const ORBITER_PAIR: [Cell; 2] = [
    Cell {
        x_base: 0,
        x_start: XStart::Peek,
        sprite: ORBITER,
        health: ORBITER_HEALTH,
        spawn_row: 0x26,
    },
    Cell {
        x_base: 0x3c,
        x_start: XStart::None,
        sprite: ORBITER,
        health: ORBITER_HEALTH,
        spawn_row: 0x27,
    },
];

const BOSS_CELL: [Cell; 1] = [Cell {
    x_base: 0,
    x_start: XStart::Peek,
    sprite: BOSS,
    health: BOSS_HEALTH,
    spawn_row: 0x45,
}];

const BOSS_AND_UPGRADES: [Cell; 6] = [
    Cell {
        x_base: 0,
        x_start: XStart::Peek,
        sprite: BOSS,
        health: BOSS_HEALTH_2,
        spawn_row: 0x46,
    },
    Cell {
        x_base: 3,
        x_start: XStart::None,
        sprite: WEAPON_UPGRADE,
        health: PICKUP_HEALTH,
        spawn_row: 0x47,
    },
    Cell {
        x_base: 0,
        x_start: XStart::None,
        sprite: WEAPON_UPGRADE,
        health: PICKUP_HEALTH,
        spawn_row: 0x48,
    },
    Cell {
        x_base: 0,
        x_start: XStart::None,
        sprite: WEAPON_UPGRADE,
        health: PICKUP_HEALTH,
        spawn_row: 0x49,
    },
    Cell {
        x_base: 0,
        x_start: XStart::None,
        sprite: WEAPON_UPGRADE,
        health: PICKUP_HEALTH,
        spawn_row: 0x4a,
    },
    Cell {
        x_base: 0,
        x_start: XStart::None,
        sprite: WEAPON_UPGRADE,
        health: PICKUP_HEALTH,
        spawn_row: 0x4b,
    },
];

const SNIPER_CELLS: [Cell; 6] = [
    Cell {
        x_base: 0x64,
        x_start: XStart::Consume,
        sprite: SNIPER,
        health: SNIPER_HEALTH,
        spawn_row: 0x21,
    },
    Cell {
        x_base: 0x28,
        x_start: XStart::None,
        sprite: SNIPER,
        health: SNIPER_HEALTH,
        spawn_row: 0x20,
    },
    Cell {
        x_base: 0,
        x_start: XStart::None,
        sprite: SNIPER,
        health: SNIPER_HEALTH,
        spawn_row: 0x22,
    },
    Cell {
        x_base: 0x28,
        x_start: XStart::None,
        sprite: SNIPER,
        health: SNIPER_HEALTH,
        spawn_row: 0x1f,
    },
    Cell {
        x_base: 0,
        x_start: XStart::None,
        sprite: SNIPER,
        health: SNIPER_HEALTH,
        spawn_row: 0x23,
    },
    Cell {
        x_base: 0x64,
        x_start: XStart::None,
        sprite: CANNON,
        health: CANNON_HEALTH,
        spawn_row: 0x16,
    },
];

// Emitter builders: each bakes one emitter's spawn constants; the dispatcher
// supplies the count (and, for asteroid_scatter, the x spread). Named for the
// enemy they emit and the emitter shape; original code addresses:
// asteroid_scatter e776, kamikaze_scatter e7bb, cannon_scatter e800,
// strafer_scatter e845, interceptor_scatter e920, sniper_scatter e965,
// interceptor_row e88a, sniper_row e8d5, cannon_or_asteroid e9aa,
// kamikaze_or_asteroid ea2d, interceptor_asteroid_row eab0, sniper_block ecbd.

fn asteroid_scatter(count: Rand, x: Rand) -> Emitter {
    Emitter::Scatter {
        count,
        x,
        sprite: ASTEROID,
        health: ASTEROID_HEALTH,
        spawn_row: rand(0x12, 0),
    }
}

fn kamikaze_scatter(count: Rand) -> Emitter {
    Emitter::Scatter {
        count,
        x: rand(0x1e, 0x1e),
        sprite: KAMIKAZE,
        health: KAMIKAZE_HEALTH,
        spawn_row: rand(5, 0x1a),
    }
}

fn cannon_scatter(count: Rand) -> Emitter {
    Emitter::Scatter {
        count,
        x: rand(0x32, 0x50),
        sprite: CANNON,
        health: CANNON_HEALTH,
        spawn_row: rand(5, 0x15),
    }
}

fn strafer_scatter(count: Rand) -> Emitter {
    Emitter::Scatter {
        count,
        x: rand(0x32, 0x78),
        sprite: STRAFER,
        health: STRAFER_HEALTH,
        spawn_row: rand(6, 0x36),
    }
}

fn interceptor_scatter(count: Rand) -> Emitter {
    Emitter::Scatter {
        count,
        x: rand(0x1e, 0x14),
        sprite: INTERCEPTOR,
        health: INTERCEPTOR_HEALTH,
        spawn_row: rand(6, 0x2c),
    }
}

fn sniper_scatter(count: Rand) -> Emitter {
    Emitter::Scatter {
        count,
        x: rand(0x1e, 0x28),
        sprite: SNIPER,
        health: SNIPER_HEALTH,
        spawn_row: rand(6, 0x1f),
    }
}

fn interceptor_row(count: Rand) -> Emitter {
    Emitter::Row {
        count,
        sprite: INTERCEPTOR,
        health: INTERCEPTOR_HEALTH,
        spawn_row: rand(4, 0x28),
        style: RowStyle::Anchored {
            x_base: 0x14,
            extra: None,
        },
    }
}

fn sniper_row(count: Rand) -> Emitter {
    Emitter::Row {
        count,
        sprite: SNIPER,
        health: SNIPER_HEALTH,
        spawn_row: rand(4, 0x32),
        style: RowStyle::Anchored {
            x_base: 0x14,
            extra: None,
        },
    }
}

fn cannon_or_asteroid(count: Rand) -> Emitter {
    Emitter::Choice {
        count,
        lo: Arm {
            x: rand(0xa, 0x1e),
            sprite: CANNON,
            health: CANNON_HEALTH,
            spawn_row: rand(5, 0x15),
        },
        hi: Arm {
            x: rand(0xa, 0x1e),
            sprite: ASTEROID,
            health: ASTEROID_HEALTH,
            spawn_row: rand(0x12, 0),
        },
    }
}

fn kamikaze_or_asteroid(count: Rand) -> Emitter {
    Emitter::Choice {
        count,
        lo: Arm {
            x: rand(0xa, 0x1e),
            sprite: KAMIKAZE,
            health: KAMIKAZE_HEALTH,
            spawn_row: rand(5, 0x1a),
        },
        hi: Arm {
            x: rand(0xa, 0x1e),
            sprite: ASTEROID,
            health: ASTEROID_HEALTH,
            spawn_row: rand(0x12, 0),
        },
    }
}

fn interceptor_asteroid_row(count: Rand) -> Emitter {
    Emitter::Row {
        count,
        sprite: INTERCEPTOR,
        health: INTERCEPTOR_HEALTH,
        spawn_row: rand(4, 0x28),
        style: RowStyle::Anchored {
            x_base: 0x14,
            extra: Some(Extra {
                sprite: ASTEROID,
                health: ASTEROID_HEALTH,
                spawn_row: rand(0x12, 0),
            }),
        },
    }
}

fn once(sprite: u16, spawn_row: Rand) -> Emitter {
    Emitter::Once {
        sprite,
        health: PICKUP_HEALTH,
        spawn_row,
    }
}

fn sniper_block(count: Rand) -> Emitter {
    Emitter::Fixed {
        repeat: Some(count),
        cells: SNIPER_CELLS.to_vec(),
    }
}

fn plain(emitter: Emitter) -> Step {
    step().emit(emitter)
}

fn at(x_start: u16, emitter: Emitter) -> Step {
    step().x_start(x_start).emit(emitter)
}

/// Returns LEVEL_1's 38-step layout script, in order.
pub fn script() -> Vec<Step> {
    vec![
        at(0x96, once(EXTRA_LIFE, rand(3, 0x42))),
        plain(asteroid_scatter(rand(7, 0x28), rand(0x1e, 0x32))),
        plain(asteroid_scatter(rand(7, 8), rand(0xa, 0x1e))),
        plain(kamikaze_or_asteroid(rand(8, 8))),
        plain(asteroid_scatter(rand(0xa, 8), rand(0xa, 0x1e))),
        at(0xc8, kamikaze_scatter(rand(3, 0xc))),
        plain(kamikaze_or_asteroid(rand(5, 0xf))),
        at(0xc8, cannon_scatter(rand(2, 6))),
        plain(cannon_or_asteroid(rand(6, 0xa))),
        at(0x28, once(SMART_BOMB, rand(3, 0x3c))),
        at(0x12c, interceptor_row(rand(5, 0xa))),
        at(
            0x12c,
            Emitter::Fixed {
                repeat: None,
                cells: ORBITER_PAIR.to_vec(),
            },
        ),
        plain(asteroid_scatter(rand(5, 5), rand(0xa, 0x1e))),
        plain(interceptor_scatter(rand(5, 8))),
        at(0x78, kamikaze_scatter(rand(0xa, 0x14))),
        at(0x28, once(INVINCIBILITY, rand(3, 0x3f))),
        at(0x14, sniper_block(rand(2, 2))),
        plain(kamikaze_scatter(rand(0xa, 0x14))),
        plain(cannon_scatter(rand(2, 6))),
        at(0x64, kamikaze_scatter(rand(5, 0xa))),
        at(0x64, sniper_scatter(rand(0xa, 0xa))),
        at(0x64, asteroid_scatter(rand(5, 5), rand(0xa, 0x1e))),
        plain(interceptor_asteroid_row(rand(5, 5))),
        plain(asteroid_scatter(rand(5, 5), rand(0xa, 0x1e))),
        plain(interceptor_asteroid_row(rand(5, 8))),
        plain(asteroid_scatter(rand(5, 0xa), rand(0xa, 0x1e))),
        at(0x64, sniper_row(rand(5, 0xa))),
        at(0xdc, strafer_scatter(rand(5, 0xa))),
        at(0x28, once(SMART_BOMB, rand(3, 0x3c))),
        at(0xdc, sniper_row(rand(5, 0xa))),
        plain(kamikaze_scatter(rand(5, 0xa))),
        at(
            0xfa,
            Emitter::Fixed {
                repeat: None,
                cells: BOSS_CELL.to_vec(),
            },
        ),
        at(0xdc, strafer_scatter(rand(5, 0xa))),
        at(0x28, once(SMART_BOMB, rand(3, 0x3c))),
        at(0xdc, sniper_row(rand(5, 0xa))),
        plain(kamikaze_scatter(rand(0x28, 0x14))),
        plain(asteroid_scatter(rand(0xa, 0xa), rand(0xa, 0x1e))),
        at(
            0xfa,
            Emitter::Fixed {
                repeat: None,
                cells: BOSS_AND_UPGRADES.to_vec(),
            },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::script;
    use crate::level::golden_hash;
    use crate::level::prng::EngineRng;
    use crate::level::slot::generate;

    /// FNV-1a over the full 446-record buffer for the validated seed.
    ///
    /// Locks the layout byte-for-byte against refactors; regenerate and re-verify
    /// against the capture (not just rebless) if it ever changes.
    const GOLDEN: &str = "9538acce1f4be2ae";

    #[test]
    fn reproduces_the_validated_capture() {
        // Seed 0x3b95 is the wall-clock seed of the captured GET-READY state.
        let records = generate(&script(), &[], &mut EngineRng::new(0x3b95));

        assert_eq!(records.len(), 446);
        assert_eq!(golden_hash(&records), GOLDEN);
    }

    #[test]
    fn is_deterministic_for_a_seed() {
        let a = generate(&script(), &[], &mut EngineRng::new(0x1234));
        let b = generate(&script(), &[], &mut EngineRng::new(0x1234));
        assert_eq!(a, b);
    }
}

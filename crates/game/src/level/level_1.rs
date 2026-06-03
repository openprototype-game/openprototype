//! LEVEL_1 (CANYON) layout data: the dispatcher script, emitter constants, and
//! depth table, transcribed from the disassembly and validated byte-for-byte
//! against the running game (seed `0x3b95` reproduces the GET-READY capture).
//! See `reference/formats/level-layout.md`.

use super::generator::{Arm, Cell, Emitter, Extra, Rand, Step, XStart, rand};

/// Per-sprite-type depth (parallax layer), read by the original from a 9-entry
/// table at `cs:[bf6d..]`.
const DEPTHS: [u16; 9] = [100, 160, 500, 600, 200, 200, 1000, 14000, 10000];

/// The foreground depth the landmark emitters hardcode (`0xfa`).
const FOREGROUND: u16 = 250;

// Fixed/repeat record blocks (the emitters with no count loop or a constant body).

const EB35: [Cell; 2] = [
    Cell {
        x_base: 0,
        x_start: XStart::Peek,
        sprite: 0x392e,
        depth: DEPTHS[6],
        y: 0x26,
    },
    Cell {
        x_base: 0x3c,
        x_start: XStart::None,
        sprite: 0x392e,
        depth: DEPTHS[6],
        y: 0x27,
    },
];

const EB72: [Cell; 1] = [Cell {
    x_base: 0,
    x_start: XStart::Peek,
    sprite: 0x3f8e,
    depth: DEPTHS[7],
    y: 0x45,
}];

const EB92: [Cell; 6] = [
    Cell {
        x_base: 0,
        x_start: XStart::Peek,
        sprite: 0x3f8e,
        depth: DEPTHS[8],
        y: 0x46,
    },
    Cell {
        x_base: 3,
        x_start: XStart::None,
        sprite: 0x36ea,
        depth: FOREGROUND,
        y: 0x47,
    },
    Cell {
        x_base: 0,
        x_start: XStart::None,
        sprite: 0x36ea,
        depth: FOREGROUND,
        y: 0x48,
    },
    Cell {
        x_base: 0,
        x_start: XStart::None,
        sprite: 0x36ea,
        depth: FOREGROUND,
        y: 0x49,
    },
    Cell {
        x_base: 0,
        x_start: XStart::None,
        sprite: 0x36ea,
        depth: FOREGROUND,
        y: 0x4a,
    },
    Cell {
        x_base: 0,
        x_start: XStart::None,
        sprite: 0x36ea,
        depth: FOREGROUND,
        y: 0x4b,
    },
];

const ECBD: [Cell; 6] = [
    Cell {
        x_base: 0x64,
        x_start: XStart::Consume,
        sprite: 0x33f4,
        depth: DEPTHS[5],
        y: 0x21,
    },
    Cell {
        x_base: 0x28,
        x_start: XStart::None,
        sprite: 0x33f4,
        depth: DEPTHS[5],
        y: 0x20,
    },
    Cell {
        x_base: 0,
        x_start: XStart::None,
        sprite: 0x33f4,
        depth: DEPTHS[5],
        y: 0x22,
    },
    Cell {
        x_base: 0x28,
        x_start: XStart::None,
        sprite: 0x33f4,
        depth: DEPTHS[5],
        y: 0x1f,
    },
    Cell {
        x_base: 0,
        x_start: XStart::None,
        sprite: 0x33f4,
        depth: DEPTHS[5],
        y: 0x23,
    },
    Cell {
        x_base: 0x64,
        x_start: XStart::None,
        sprite: 0x338e,
        depth: DEPTHS[2],
        y: 0x16,
    },
];

// Emitter builders: each bakes one emitter's scenery constants; the dispatcher
// supplies the count (and, for e776, the x spread). Named for their original
// code addresses.

fn e776(count: Rand, x: Rand) -> Emitter {
    Emitter::Single {
        count,
        x,
        sprite: 0x3308,
        depth: DEPTHS[0],
        y: rand(0x12, 0),
    }
}

fn e7bb(count: Rand) -> Emitter {
    Emitter::Single {
        count,
        x: rand(0x1e, 0x1e),
        sprite: 0x38b0,
        depth: DEPTHS[1],
        y: rand(5, 0x1a),
    }
}

fn e800(count: Rand) -> Emitter {
    Emitter::Single {
        count,
        x: rand(0x32, 0x50),
        sprite: 0x338e,
        depth: DEPTHS[2],
        y: rand(5, 0x15),
    }
}

fn e845(count: Rand) -> Emitter {
    Emitter::Single {
        count,
        x: rand(0x32, 0x78),
        sprite: 0x39a4,
        depth: DEPTHS[3],
        y: rand(6, 0x36),
    }
}

fn e920(count: Rand) -> Emitter {
    Emitter::Single {
        count,
        x: rand(0x1e, 0x14),
        sprite: 0x3a92,
        depth: DEPTHS[4],
        y: rand(6, 0x2c),
    }
}

fn e965(count: Rand) -> Emitter {
    Emitter::Single {
        count,
        x: rand(0x1e, 0x28),
        sprite: 0x33f4,
        depth: DEPTHS[5],
        y: rand(6, 0x1f),
    }
}

fn e88a(count: Rand) -> Emitter {
    Emitter::Row {
        count,
        x_base: 0x14,
        sprite: 0x3a92,
        depth: DEPTHS[4],
        y_once: rand(4, 0x28),
    }
}

fn e8d5(count: Rand) -> Emitter {
    Emitter::Row {
        count,
        x_base: 0x14,
        sprite: 0x33f4,
        depth: DEPTHS[5],
        y_once: rand(4, 0x32),
    }
}

fn e9aa(count: Rand) -> Emitter {
    Emitter::Choice {
        count,
        lo: Arm {
            x: rand(0xa, 0x1e),
            sprite: 0x338e,
            depth: DEPTHS[2],
            y: rand(5, 0x15),
        },
        hi: Arm {
            x: rand(0xa, 0x1e),
            sprite: 0x3308,
            depth: DEPTHS[0],
            y: rand(0x12, 0),
        },
    }
}

fn ea2d(count: Rand) -> Emitter {
    Emitter::Choice {
        count,
        lo: Arm {
            x: rand(0xa, 0x1e),
            sprite: 0x38b0,
            depth: DEPTHS[1],
            y: rand(5, 0x1a),
        },
        hi: Arm {
            x: rand(0xa, 0x1e),
            sprite: 0x3308,
            depth: DEPTHS[0],
            y: rand(0x12, 0),
        },
    }
}

fn eab0(count: Rand) -> Emitter {
    Emitter::RowEveryNth {
        count,
        x_base: 0x14,
        sprite: 0x3a92,
        depth: DEPTHS[4],
        y_once: rand(4, 0x28),
        extra: Extra {
            sprite: 0x3308,
            depth: DEPTHS[0],
            y: rand(0x12, 0),
        },
    }
}

fn once(sprite: u16, y: Rand) -> Emitter {
    Emitter::Once {
        sprite,
        depth: FOREGROUND,
        y,
    }
}

fn ecbd(count: Rand) -> Emitter {
    Emitter::Repeat {
        count,
        block: &ECBD,
    }
}

fn step(emitter: Emitter) -> Step {
    Step {
        set_x_start: None,
        emitter,
    }
}

fn step_at(x_start: u16, emitter: Emitter) -> Step {
    Step {
        set_x_start: Some(x_start),
        emitter,
    }
}

/// LEVEL_1's 38-step layout script, in order.
pub fn script() -> Vec<Step> {
    vec![
        step_at(0x96, once(0x382c, rand(3, 0x42))),
        step(e776(rand(7, 0x28), rand(0x1e, 0x32))),
        step(e776(rand(7, 8), rand(0xa, 0x1e))),
        step(ea2d(rand(8, 8))),
        step(e776(rand(0xa, 8), rand(0xa, 0x1e))),
        step_at(0xc8, e7bb(rand(3, 0xc))),
        step(ea2d(rand(5, 0xf))),
        step_at(0xc8, e800(rand(2, 6))),
        step(e9aa(rand(6, 0xa))),
        step_at(0x28, once(0x3750, rand(3, 0x3c))),
        step_at(0x12c, e88a(rand(5, 0xa))),
        step_at(0x12c, Emitter::Fixed(&EB35)),
        step(e776(rand(5, 5), rand(0xa, 0x1e))),
        step(e920(rand(5, 8))),
        step_at(0x78, e7bb(rand(0xa, 0x14))),
        step_at(0x28, once(0x37b6, rand(3, 0x3f))),
        step_at(0x14, ecbd(rand(2, 2))),
        step(e7bb(rand(0xa, 0x14))),
        step(e800(rand(2, 6))),
        step_at(0x64, e7bb(rand(5, 0xa))),
        step_at(0x64, e965(rand(0xa, 0xa))),
        step_at(0x64, e776(rand(5, 5), rand(0xa, 0x1e))),
        step(eab0(rand(5, 5))),
        step(e776(rand(5, 5), rand(0xa, 0x1e))),
        step(eab0(rand(5, 8))),
        step(e776(rand(5, 0xa), rand(0xa, 0x1e))),
        step_at(0x64, e8d5(rand(5, 0xa))),
        step_at(0xdc, e845(rand(5, 0xa))),
        step_at(0x28, once(0x3750, rand(3, 0x3c))),
        step_at(0xdc, e8d5(rand(5, 0xa))),
        step(e7bb(rand(5, 0xa))),
        step_at(0xfa, Emitter::Fixed(&EB72)),
        step_at(0xdc, e845(rand(5, 0xa))),
        step_at(0x28, once(0x3750, rand(3, 0x3c))),
        step_at(0xdc, e8d5(rand(5, 0xa))),
        step(e7bb(rand(0x28, 0x14))),
        step(e776(rand(0xa, 0xa), rand(0xa, 0x1e))),
        step_at(0xfa, Emitter::Fixed(&EB92)),
    ]
}

#[cfg(test)]
mod tests {
    use super::script;
    use crate::level::generator::generate;
    use crate::level::golden_hash;
    use crate::level::prng::EngineRng;

    /// FNV-1a over the full 446-record buffer for the validated seed. Locks the
    /// layout byte-for-byte against refactors; regenerate and re-verify against
    /// the capture (not just rebless) if it ever changes.
    const GOLDEN: &str = "9538acce1f4be2ae";

    #[test]
    fn reproduces_the_validated_capture() {
        // Seed 0x3b95 is the wall-clock seed of the captured GET-READY state.
        let records = generate(&script(), &mut EngineRng::new(0x3b95));

        assert_eq!(records.len(), 446);
        assert_eq!(golden_hash(&records), GOLDEN);
    }

    #[test]
    fn is_deterministic_for_a_seed() {
        let a = generate(&script(), &mut EngineRng::new(0x1234));
        let b = generate(&script(), &mut EngineRng::new(0x1234));
        assert_eq!(a, b);
    }
}

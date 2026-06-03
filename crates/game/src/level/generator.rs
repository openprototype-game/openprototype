//! The layout-script interpreter.
//!
//! A level's object layout is a straight-line script of placement steps, each
//! invoking one emitter that appends 8-byte records to a growing buffer. The
//! emitters share a running x-start and the engine PRNG, so the draw order is
//! load-bearing: it must match the original exactly. This interpreter mirrors
//! the disassembly validated byte-for-byte against the running game (seed
//! `0x3b95` reproduces LEVEL_1's GET-READY capture). See
//! `reference/formats/level-layout.md`.

use super::prng::EngineRng;

/// One placed object. `x_step` is a horizontal step (the consumer running-sums
/// these to an absolute scroll position); `depth` is the parallax layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub x_step: u16,
    pub sprite: u16,
    pub depth: u16,
    pub y: u16,
}

/// A bounded draw: `rng(modulus) + base`. Every layout draw passes a nonzero
/// modulus.
#[derive(Clone, Copy)]
pub struct Rand {
    pub modulus: u16,
    pub base: u16,
}

pub const fn rand(modulus: u16, base: u16) -> Rand {
    Rand { modulus, base }
}

/// A scatter spec for a `Choice` arm: an rng x and an rng y.
#[derive(Clone, Copy)]
pub struct Arm {
    pub x: Rand,
    pub sprite: u16,
    pub depth: u16,
    pub y: Rand,
}

/// The extra object a `RowEveryNth` inserts every second record (always x = 0).
#[derive(Clone, Copy)]
pub struct Extra {
    pub sprite: u16,
    pub depth: u16,
    pub y: Rand,
}

/// How a fixed cell uses the running x-start.
#[derive(Clone, Copy)]
pub enum XStart {
    /// Ignore it (x = base).
    None,
    /// Add it, then zero it (the common consume).
    Consume,
    /// Add it but leave it set (the Fixed landmark emitters).
    Peek,
}

/// One record of a no-count block (`Fixed` / `Repeat`). Always a constant y.
#[derive(Clone, Copy)]
pub struct Cell {
    pub x_base: u16,
    pub x_start: XStart,
    pub sprite: u16,
    pub depth: u16,
    pub y: u16,
}

/// The emitter kinds. Each has a fixed PRNG draw sequence (encoded in [`run`]).
pub enum Emitter {
    /// `count = rng+`; per record: x = rng + base + xstart, y = rng.
    Single {
        count: Rand,
        x: Rand,
        sprite: u16,
        depth: u16,
        y: Rand,
    },
    /// y drawn once before the count; per record: x = base + xstart, y shared.
    Row {
        count: Rand,
        x_base: u16,
        sprite: u16,
        depth: u16,
        y_once: Rand,
    },
    /// `count`; per record: `rng(5) > 1 ? hi : lo`.
    Choice { count: Rand, lo: Arm, hi: Arm },
    /// A `Row` that also inserts an extra object every second record.
    RowEveryNth {
        count: Rand,
        x_base: u16,
        sprite: u16,
        depth: u16,
        y_once: Rand,
        extra: Extra,
    },
    /// One record, count ignored: x = xstart, y = rng.
    Once { sprite: u16, depth: u16, y: Rand },
    /// A fixed handful of records, no count loop.
    Fixed(&'static [Cell]),
    /// `count`; each iteration emits a fixed block of records.
    Repeat { count: Rand, block: &'static [Cell] },
}

/// One dispatcher step: optionally set the running x-start, then run an emitter.
pub struct Step {
    pub set_x_start: Option<u16>,
    pub emitter: Emitter,
}

/// Run a level's script against a seeded PRNG, producing its object records.
pub fn generate(script: &[Step], rng: &mut EngineRng) -> Vec<Record> {
    let mut out = Vec::new();
    let mut x_start = 0u16;

    for step in script {
        if let Some(x) = step.set_x_start {
            x_start = x;
        }

        run(&step.emitter, rng, &mut x_start, &mut out);
    }

    out
}

fn draw(rng: &mut EngineRng, r: Rand) -> u16 {
    rng.next(r.modulus).wrapping_add(r.base)
}

/// Read the running x-start and zero it (the common per-band consume).
fn take(x_start: &mut u16) -> u16 {
    let value = *x_start;
    *x_start = 0;
    value
}

fn emit_cell(cell: &Cell, x_start: &mut u16, out: &mut Vec<Record>) {
    let xs = match cell.x_start {
        XStart::None => 0,
        XStart::Consume => take(x_start),
        XStart::Peek => *x_start,
    };

    out.push(Record {
        x_step: cell.x_base.wrapping_add(xs),
        sprite: cell.sprite,
        depth: cell.depth,
        y: cell.y,
    });
}

fn run(emitter: &Emitter, rng: &mut EngineRng, x_start: &mut u16, out: &mut Vec<Record>) {
    match emitter {
        Emitter::Single {
            count,
            x,
            sprite,
            depth,
            y,
        } => {
            let n = rng.next(count.modulus).wrapping_add(count.base);

            for _ in 0..n {
                // x draws before y; the x-start is consumed during x.
                let x_step = draw(rng, *x).wrapping_add(take(x_start));
                let y = draw(rng, *y);
                out.push(Record {
                    x_step,
                    sprite: *sprite,
                    depth: *depth,
                    y,
                });
            }
        }

        Emitter::Row {
            count,
            x_base,
            sprite,
            depth,
            y_once,
        } => {
            // The shared y is drawn once, before the count.
            let y = draw(rng, *y_once);
            let n = rng.next(count.modulus).wrapping_add(count.base);

            for _ in 0..n {
                let x_step = x_base.wrapping_add(take(x_start));
                out.push(Record {
                    x_step,
                    sprite: *sprite,
                    depth: *depth,
                    y,
                });
            }
        }

        Emitter::Choice { count, lo, hi } => {
            let n = rng.next(count.modulus).wrapping_add(count.base);

            for _ in 0..n {
                let arm = if rng.next(5) > 1 { hi } else { lo };
                let x_step = draw(rng, arm.x).wrapping_add(take(x_start));
                let y = draw(rng, arm.y);
                out.push(Record {
                    x_step,
                    sprite: arm.sprite,
                    depth: arm.depth,
                    y,
                });
            }
        }

        Emitter::RowEveryNth {
            count,
            x_base,
            sprite,
            depth,
            y_once,
            extra,
        } => {
            let y = draw(rng, *y_once);
            let n = rng.next(count.modulus).wrapping_add(count.base);
            let mut counter = 0u16;

            for _ in 0..n {
                let x_step = x_base.wrapping_add(take(x_start));
                out.push(Record {
                    x_step,
                    sprite: *sprite,
                    depth: *depth,
                    y,
                });
                counter += 1;

                if counter == 2 {
                    counter = 0;
                    let y = draw(rng, extra.y);
                    out.push(Record {
                        x_step: 0,
                        sprite: extra.sprite,
                        depth: extra.depth,
                        y,
                    });
                }
            }
        }

        Emitter::Once { sprite, depth, y } => {
            let x_step = take(x_start);
            let y = draw(rng, *y);
            out.push(Record {
                x_step,
                sprite: *sprite,
                depth: *depth,
                y,
            });
        }

        Emitter::Fixed(cells) => {
            for cell in *cells {
                emit_cell(cell, x_start, out);
            }
        }

        Emitter::Repeat { count, block } => {
            let n = rng.next(count.modulus).wrapping_add(count.base);

            for _ in 0..n {
                for cell in *block {
                    emit_cell(cell, x_start, out);
                }
            }
        }
    }
}

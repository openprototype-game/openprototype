//! The slot-based layout interpreter (generated levels 3/5/7).
//!
//! Unlike LEVEL_1's emitters, which bake their scenery constants, the later
//! levels drive *generic* emitters: the dispatcher writes a handful of engine
//! slots (sprite, depth, x-step, row-reset, row-y offset) before each call, and
//! the emitters read them. Some slots persist across steps. The script also ends
//! with a find-by-position post-pass that rewrites already-emitted records in
//! place. This interpreter mirrors the disassembly, validated byte-for-byte
//! against the running game (LEVEL_3 seed `0x1a94` reproduces its GET-READY
//! capture). See `reference/formats/level-layout.md`.
//!
//! LEVEL_1 still uses the baked [`super::generator`] model; both fold into this
//! one once LEVEL_7 is in (the generated-level set is then complete).

use super::generator::{Rand, Record};
use super::prng::EngineRng;

/// The mutable engine slots the dispatcher writes and the emitters read.
#[derive(Default)]
struct Slots {
    x_start: u16,
    x_step: u16,
    sprite: u16,
    depth: u16,
    row_reset: u16,
    row_y_offset: u16,
}

/// The slot writes a step performs before running its emitter. Only the fields
/// the original sets are `Some`; the rest carry over from earlier steps.
#[derive(Default, Clone, Copy)]
pub struct SlotPatch {
    pub x_start: Option<u16>,
    pub x_step: Option<u16>,
    pub sprite: Option<u16>,
    pub depth: Option<u16>,
    pub row_reset: Option<u16>,
    pub row_y_offset: Option<u16>,
}

impl Slots {
    fn apply(&mut self, patch: &SlotPatch) {
        if let Some(v) = patch.x_start {
            self.x_start = v;
        }

        if let Some(v) = patch.x_step {
            self.x_step = v;
        }

        if let Some(v) = patch.sprite {
            self.sprite = v;
        }

        if let Some(v) = patch.depth {
            self.depth = v;
        }

        if let Some(v) = patch.row_reset {
            self.row_reset = v;
        }

        if let Some(v) = patch.row_y_offset {
            self.row_y_offset = v;
        }
    }

    /// x = x_start + x_step, then consume x_start (the common per-record step).
    fn step_x(&mut self) -> u16 {
        let x = self.x_start.wrapping_add(self.x_step);
        self.x_start = 0;
        x
    }

    /// x = x_start, then consume it (the landmark `Once` emitters; no step).
    fn consume_x(&mut self) -> u16 {
        let x = self.x_start;
        self.x_start = 0;
        x
    }
}

/// The emitter kinds, named for the original routine each transcribes.
pub enum Emitter {
    /// `0x121e7`/`12213`/`1223f`: one record, x = x_start, fixed `depth`.
    Once { sprite: u16, depth: u16, y: Rand },
    /// `0x1226b`/`122ab`: count loop, x = x_start + x_step (no x draw), with a
    /// hardcoded sprite/depth.
    Single {
        count: Rand,
        sprite: u16,
        depth: u16,
        y: Rand,
    },
    /// `0x123de`/`12434`: like `Single` but the sprite/depth come from the slots,
    /// and one dead `rng(0xa)` row-y draw is burned before the count loop.
    SlotSingle { count: Rand, y: Rand },
    /// `0x122eb`/`12367`: `outer` rows of `inner` records (slot sprite/depth). A
    /// row-y is drawn once per row; `0` inner modulus skips the inner-count draw.
    /// After each row the x-start resets to `row_reset`.
    Grid {
        outer: Rand,
        inner: Rand,
        row_y: Rand,
        row_y_uses_offset: bool,
    },
    /// `0x1248a`: a fixed lead (rec0 x = x_start + x_step; rec1 x = `mid.0`) then
    /// `count` trailing records at `run.0`. No PRNG draws.
    FixedRun {
        sprite: u16,
        depth: u16,
        y_lead: u16,
        mid: (u16, u16),
        run: (u16, u16),
        count: u16,
    },
}

/// One dispatcher step: write some slots, then run an emitter.
pub struct Step {
    pub set: SlotPatch,
    pub emitter: Emitter,
}

/// A find-by-position overwrite (`0x12c26` + a half-emitter): walk the buffer
/// summing x-steps, find the record covering `target_x`, rewrite its x-step, and
/// replace its sprite/depth/y. Mutates an already-built record.
pub struct Overwrite {
    pub target_x: u16,
    pub sprite: u16,
    pub depth: u16,
    pub y: u16,
}

/// A fluent builder for a [`Step`]; keeps the slot writes readable at the script
/// call sites.
pub struct StepBuilder {
    set: SlotPatch,
}

pub fn step() -> StepBuilder {
    StepBuilder {
        set: SlotPatch::default(),
    }
}

impl StepBuilder {
    pub fn x_start(mut self, v: u16) -> Self {
        self.set.x_start = Some(v);
        self
    }

    pub fn x_step(mut self, v: u16) -> Self {
        self.set.x_step = Some(v);
        self
    }

    pub fn sprite(mut self, v: u16) -> Self {
        self.set.sprite = Some(v);
        self
    }

    pub fn depth(mut self, v: u16) -> Self {
        self.set.depth = Some(v);
        self
    }

    pub fn row_reset(mut self, v: u16) -> Self {
        self.set.row_reset = Some(v);
        self
    }

    pub fn row_y_offset(mut self, v: u16) -> Self {
        self.set.row_y_offset = Some(v);
        self
    }

    pub fn emit(self, emitter: Emitter) -> Step {
        Step {
            set: self.set,
            emitter,
        }
    }
}

fn draw(rng: &mut EngineRng, r: Rand) -> u16 {
    rng.next(r.modulus).wrapping_add(r.base)
}

/// Run a slot-model script against a seeded PRNG, then apply the post-pass.
pub fn generate(script: &[Step], overwrites: &[Overwrite], rng: &mut EngineRng) -> Vec<Record> {
    let mut slots = Slots::default();
    let mut out = Vec::new();

    for step in script {
        slots.apply(&step.set);
        run(&step.emitter, &mut slots, rng, &mut out);
    }

    for overwrite in overwrites {
        apply_overwrite(overwrite, &mut out);
    }

    out
}

fn run(emitter: &Emitter, slots: &mut Slots, rng: &mut EngineRng, out: &mut Vec<Record>) {
    match emitter {
        Emitter::Once { sprite, depth, y } => {
            out.push(Record {
                x_step: slots.consume_x(),
                sprite: *sprite,
                depth: *depth,
                y: draw(rng, *y),
            });
        }

        Emitter::Single {
            count,
            sprite,
            depth,
            y,
        } => {
            let n = draw(rng, *count);

            for _ in 0..n {
                out.push(Record {
                    x_step: slots.step_x(),
                    sprite: *sprite,
                    depth: *depth,
                    y: draw(rng, *y),
                });
            }
        }

        Emitter::SlotSingle { count, y } => {
            let n = draw(rng, *count);
            rng.next(0xa); // dead row-y draw (computed but unused here)

            for _ in 0..n {
                out.push(Record {
                    x_step: slots.step_x(),
                    sprite: slots.sprite,
                    depth: slots.depth,
                    y: draw(rng, *y),
                });
            }
        }

        Emitter::Grid {
            outer,
            inner,
            row_y,
            row_y_uses_offset,
        } => {
            let rows = draw(rng, *outer);

            for _ in 0..rows {
                // A zero inner modulus means a fixed inner count with no draw.
                let cols = if inner.modulus == 0 {
                    inner.base
                } else {
                    draw(rng, *inner)
                };

                let mut y = draw(rng, *row_y);

                if *row_y_uses_offset {
                    y = y.wrapping_add(slots.row_y_offset);
                }

                for _ in 0..cols {
                    out.push(Record {
                        x_step: slots.step_x(),
                        sprite: slots.sprite,
                        depth: slots.depth,
                        y,
                    });
                }

                slots.x_start = slots.row_reset;
            }
        }

        Emitter::FixedRun {
            sprite,
            depth,
            y_lead,
            mid,
            run,
            count,
        } => {
            out.push(Record {
                x_step: slots.step_x(),
                sprite: *sprite,
                depth: *depth,
                y: *y_lead,
            });
            out.push(Record {
                x_step: mid.0,
                sprite: *sprite,
                depth: *depth,
                y: mid.1,
            });

            for _ in 0..*count {
                out.push(Record {
                    x_step: run.0,
                    sprite: *sprite,
                    depth: *depth,
                    y: run.1,
                });
            }
        }
    }
}

fn apply_overwrite(overwrite: &Overwrite, out: &mut [Record]) {
    let mut cumulative = 0u16;

    for record in out.iter_mut() {
        let before = cumulative;
        cumulative = cumulative.wrapping_add(record.x_step);

        if cumulative > overwrite.target_x {
            record.x_step = overwrite.target_x.wrapping_sub(before);
            record.sprite = overwrite.sprite;
            record.depth = overwrite.depth;
            record.y = overwrite.y;
            return;
        }
    }
}

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
    /// `0xfd82`/`0xff16`: a run of `count` records (`count = rng + base`) sharing
    /// one y, drawn once before the loop. x = x_start + x_step; sprite/depth are
    /// hardcoded.
    Row {
        count: Rand,
        sprite: u16,
        depth: u16,
        y: Rand,
    },
    /// `0xffe0`: `rows` rows (`rows = rng + base`). A single `rng(3)` picks the
    /// y-pair shared by every row — result `1` selects `if_one`, anything else
    /// `otherwise`. Each row emits two records: the first x = x_start + x_step,
    /// the second x = 0. sprite/depth hardcoded.
    BranchRows {
        rows: Rand,
        sprite: u16,
        depth: u16,
        if_one: (u16, u16),
        otherwise: (u16, u16),
    },
    /// `0x10119`/`0x10146` (L5), `0x1222f`/`0x1224e`/`0x12289` (L7): a fixed run
    /// with no PRNG draws. The lead record's x is x_start + x_step when `lead_step`
    /// (L5), or x_start alone when not (L7's landmark blocks); each `rest` record
    /// carries its own literal `(x, sprite, depth, y)`.
    Fixed {
        lead_step: bool,
        lead_sprite: u16,
        lead_depth: u16,
        lead_y: u16,
        rest: Vec<(u16, u16, u16, u16)>,
    },
}

/// One dispatcher step: write some slots, then run an emitter. When `repeat` is
/// set, the count is drawn once (before the slot writes) and the slot-write +
/// emitter body runs that many times — a loop the dispatcher builds around a
/// `call`.
pub struct Step {
    pub set: SlotPatch,
    pub emitter: Emitter,
    pub repeat: Option<Rand>,
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

/// A find-by-position insert (L7's `0x12381` + fill): walk the buffer summing
/// x-steps, find the record covering `target_x`, open a one-record gap there
/// (splitting that record's x-step into `target_x - before` / `after - target_x`),
/// then write `records` at the gap. The first entry fills the inserted slot (its
/// split x-step is kept); each later entry overwrites the following record with
/// `x = 0`. Mirrors the original's `rep movsb` buffer shift and the 5-record
/// template / single-landmark fills.
pub struct Insert {
    pub target_x: u16,
    pub records: Vec<(u16, u16, u16)>,
}

/// A post-pass step: either an in-place overwrite (L3) or a buffer-shifting
/// insert (L7).
pub enum PostOp {
    Overwrite(Overwrite),
    Insert(Insert),
}

/// The generated buffer's fixed capacity (`(0x2c3a - 0xd02) / 8`); an insert past
/// it drops the tail, matching the original's bounded `rep movsb`.
const BUFFER_CAPACITY: usize = (0x2c3a - 0xd02) / 8;

/// A fluent builder for a [`Step`]; keeps the slot writes readable at the script
/// call sites.
pub struct StepBuilder {
    set: SlotPatch,
    repeat: Option<Rand>,
}

pub fn step() -> StepBuilder {
    StepBuilder {
        set: SlotPatch::default(),
        repeat: None,
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

    /// Repeat this step `rng(count) + base` times (a dispatcher-level loop).
    pub fn repeat(mut self, count: Rand) -> Self {
        self.repeat = Some(count);
        self
    }

    pub fn emit(self, emitter: Emitter) -> Step {
        Step {
            set: self.set,
            emitter,
            repeat: self.repeat,
        }
    }
}

fn draw(rng: &mut EngineRng, r: Rand) -> u16 {
    rng.next(r.modulus).wrapping_add(r.base)
}

/// Run a slot-model script against a seeded PRNG, then apply the post-pass.
pub fn generate(script: &[Step], post: &[PostOp], rng: &mut EngineRng) -> Vec<Record> {
    let mut slots = Slots::default();
    let mut out = Vec::new();

    for step in script {
        // A repeated step draws its count once, then re-applies its slot writes
        // and runs its emitter each iteration (the dispatcher's `call` loop).
        let times = match step.repeat {
            Some(count) => draw(rng, count),
            None => 1,
        };

        for _ in 0..times {
            slots.apply(&step.set);
            run(&step.emitter, &mut slots, rng, &mut out);
        }
    }

    for op in post {
        match op {
            PostOp::Overwrite(overwrite) => apply_overwrite(overwrite, &mut out),
            PostOp::Insert(insert) => apply_insert(insert, &mut out),
        }
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

        Emitter::Row {
            count,
            sprite,
            depth,
            y,
        } => {
            let n = draw(rng, *count);
            let row_y = draw(rng, *y); // one y for the whole run

            for _ in 0..n {
                out.push(Record {
                    x_step: slots.step_x(),
                    sprite: *sprite,
                    depth: *depth,
                    y: row_y,
                });
            }
        }

        Emitter::BranchRows {
            rows,
            sprite,
            depth,
            if_one,
            otherwise,
        } => {
            let n = draw(rng, *rows);
            // One rng(3) picks the shared y-pair for every row.
            let (y_lead, y_tail) = if rng.next(3) == 1 {
                *if_one
            } else {
                *otherwise
            };

            for _ in 0..n {
                out.push(Record {
                    x_step: slots.step_x(),
                    sprite: *sprite,
                    depth: *depth,
                    y: y_lead,
                });
                out.push(Record {
                    x_step: 0,
                    sprite: *sprite,
                    depth: *depth,
                    y: y_tail,
                });
            }
        }

        Emitter::Fixed {
            lead_step,
            lead_sprite,
            lead_depth,
            lead_y,
            rest,
        } => {
            let x_step = if *lead_step {
                slots.step_x()
            } else {
                slots.consume_x()
            };

            out.push(Record {
                x_step,
                sprite: *lead_sprite,
                depth: *lead_depth,
                y: *lead_y,
            });

            for &(x_step, sprite, depth, y) in rest {
                out.push(Record {
                    x_step,
                    sprite,
                    depth,
                    y,
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

fn apply_insert(insert: &Insert, out: &mut Vec<Record>) {
    let mut cumulative = 0u16;

    for i in 0..out.len() {
        let before = cumulative;
        cumulative = cumulative.wrapping_add(out[i].x_step);

        if cumulative > insert.target_x {
            // Split the covered record's x-step across the new gap, then shift the
            // tail up one slot (dropping anything past the buffer capacity).
            let new_x_step = insert.target_x.wrapping_sub(before);
            let remainder_x_step = cumulative.wrapping_sub(insert.target_x);

            out.insert(
                i,
                Record {
                    x_step: new_x_step,
                    sprite: 0,
                    depth: 0,
                    y: 0,
                },
            );
            out[i + 1].x_step = remainder_x_step;
            out.truncate(BUFFER_CAPACITY);

            // First entry fills the inserted slot (keeping its split x-step); each
            // later entry overwrites the following record at x = 0.
            let (sprite, depth, y) = insert.records[0];
            out[i].sprite = sprite;
            out[i].depth = depth;
            out[i].y = y;

            for (offset, &(sprite, depth, y)) in insert.records[1..].iter().enumerate() {
                out[i + 1 + offset] = Record {
                    x_step: 0,
                    sprite,
                    depth,
                    y,
                };
            }

            return;
        }
    }
}

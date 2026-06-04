//! The layout interpreter for all four generated levels (1, 3, 5, 7).
//!
//! Every generated level is a straight-line script of placement steps, each
//! running one emitter that appends 8-byte records to a growing buffer, sharing
//! the engine PRNG so the draw order is load-bearing. Most emitters bake their
//! sprite/depth into the variant; a couple ([`Steps`](Emitter::Steps) with
//! [`Fill::Slots`] and [`Grid`](Emitter::Grid)) instead read them from engine
//! slots the dispatcher writes between steps and that persist across them. A
//! script may end with a post-pass that either overwrites a record in
//! place (LEVEL_3) or inserts one by splitting the covered record's x-step
//! (LEVEL_7). This mirrors the disassembly, validated byte-for-byte against the
//! running game (each level's golden test reproduces its GET-READY capture). See
//! `reference/formats/level-layout.md`.

use super::prng::EngineRng;

/// One placed scenery object.
///
/// `x_step` is a horizontal step; a consumer running-sums these into an
/// absolute scroll position. `depth` is the parallax layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub x_step: u16,
    pub sprite: u16,
    pub depth: u16,
    pub y: u16,
}

/// A bounded PRNG draw, `rng(modulus) + base`.
///
/// Every layout draw passes a nonzero modulus.
#[derive(Clone, Copy)]
pub struct Rand {
    pub modulus: u16,
    pub base: u16,
}

/// Builds a [`Rand`] from a modulus and a base.
pub const fn rand(modulus: u16, base: u16) -> Rand {
    Rand { modulus, base }
}

/// One arm of a [`Choice`](Emitter::Choice): a sprite/depth with rng x and y.
#[derive(Clone, Copy)]
pub struct Arm {
    pub x: Rand,
    pub sprite: u16,
    pub depth: u16,
    pub y: Rand,
}

/// The object a [`Row`](Emitter::Row) inserts after every second record.
///
/// It is always placed at x = 0.
#[derive(Clone, Copy)]
pub struct Extra {
    pub sprite: u16,
    pub depth: u16,
    pub y: Rand,
}

/// Where a [`Steps`](Emitter::Steps) emitter gets each record's sprite/depth.
#[derive(Clone, Copy)]
pub enum Fill {
    /// Hardcoded in the emitter.
    Baked { sprite: u16, depth: u16 },
    /// From the engine slots the dispatcher wrote.
    ///
    /// The original's slot-reading routine also computes a row-y it never
    /// uses, so this form burns one vestigial `rng(0xa)` draw before the
    /// loop (load-bearing for draw order).
    Slots,
}

/// How a [`Row`](Emitter::Row) lays out records and draws its shared y.
///
/// The two forms are different relinked routines, each with its own draw
/// order, so they are not interchangeable.
#[derive(Clone, Copy)]
pub enum RowStyle {
    /// Draws the shared y after the count; x = x_start + x_step.
    Stepped,
    /// Draws the shared y before the count; x = x_base + x_start.
    ///
    /// `extra` inserts an object (x = 0) after every second record.
    Anchored { x_base: u16, extra: Option<Extra> },
}

/// How a [`Cell`] uses the running x-start.
#[derive(Clone, Copy)]
pub enum XStart {
    /// Ignore it (x = base).
    None,
    /// Add it, then zero it (the common consume).
    Consume,
    /// Add it but leave it set (the landmark leads that peek).
    Peek,
    /// Add x_start + x_step, then zero x_start (a stepped lead).
    Step,
}

/// One record of a [`Fixed`](Emitter::Fixed) block.
///
/// Always a constant y; the `x_start` mode says how it uses the running
/// x-start.
#[derive(Clone, Copy)]
pub struct Cell {
    pub x_base: u16,
    pub x_start: XStart,
    pub sprite: u16,
    pub depth: u16,
    pub y: u16,
}

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

/// A step's writes to the engine slots, applied before its emitter runs.
///
/// Only the fields the original sets are `Some`; the rest carry over from
/// earlier steps.
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

/// The placement emitters a step can run.
///
/// Each has a fixed PRNG draw sequence (see the `run` dispatch) and a
/// record-placement rule; its doc notes the original routine it mirrors.
pub enum Emitter {
    /// Emits one record at x = x_start with a fixed sprite/depth.
    ///
    /// (orig `0x121e7`/`12213`/`1223f`.)
    Once { sprite: u16, depth: u16, y: Rand },
    /// Emits `count` stepped records (x = x_start + x_step), random y each.
    ///
    /// `fill` is the sprite/depth source: [`Baked`](Fill::Baked) hardcodes
    /// them, [`Slots`](Fill::Slots) reads them from the engine slots and burns
    /// one vestigial draw. (orig `0x1226b`/`122ab`; `Slots` form
    /// `0x123de`/`12434`.)
    Steps { count: Rand, y: Rand, fill: Fill },
    /// Emits `outer` rows of `inner` records, with slot sprite/depth.
    ///
    /// A row-y is drawn once per row; a zero `inner` modulus uses a fixed
    /// inner count with no draw. After each row the x-start resets to
    /// `row_reset`. (orig `0x122eb`/`12367`.)
    Grid {
        outer: Rand,
        inner: Rand,
        row_y: Rand,
        row_y_uses_offset: bool,
    },
    /// Emits `count` records sharing one drawn y, laid out per `style`.
    ///
    /// See [`RowStyle`] for the x rule and the y draw order; sprite/depth are
    /// hardcoded. (orig `0xfd82`/`0xff16`; anchored forms
    /// `0xe88a`/`0xe8d5`/`0xeab0`.)
    Row {
        count: Rand,
        sprite: u16,
        depth: u16,
        y: Rand,
        style: RowStyle,
    },
    /// Emits `rows` rows of two records at a y-pair picked by one `rng(3)`.
    ///
    /// Result `1` selects `pair_when_one`, anything else `pair_otherwise`. Per
    /// row the lead's x = x_start + x_step and the tail's x = 0; sprite/depth
    /// are hardcoded. (orig `0xffe0`.)
    PairedRows {
        rows: Rand,
        sprite: u16,
        depth: u16,
        pair_when_one: (u16, u16),
        pair_otherwise: (u16, u16),
    },
    /// Emits a list of literal `cells` with no per-record draw.
    ///
    /// Runs once when `repeat` is `None`, or `rng(repeat) + base` times when
    /// `Some` — this emitter's own loop, not the dispatcher-level
    /// [`Step::repeat`]. Each cell's `x_start` says how it touches the running
    /// x-start. (orig L1 `0xeb35`/`eb72`/`eb92`/`ecbd`, L3 `0x1248a`, L5/L7
    /// landmark blocks.)
    Fixed {
        repeat: Option<Rand>,
        cells: Vec<Cell>,
    },
    /// Emits `count` records, each at a per-record drawn x with a random y.
    ///
    /// x = `rng(x) + x_start` (consume, no x-step); the per-record x draw is
    /// what sets it apart from [`Steps`](Emitter::Steps). (orig LEVEL_1's
    /// `0xe776` family.)
    Scatter {
        count: Rand,
        x: Rand,
        sprite: u16,
        depth: u16,
        y: Rand,
    },
    /// Emits `count` records, each picking `rng(5) > 1 ? hi : lo`.
    ///
    /// (orig LEVEL_1's `0xe9aa`/`0xea2d`.)
    Choice { count: Rand, lo: Arm, hi: Arm },
}

/// One dispatcher step: write some slots, then run an emitter.
///
/// When `repeat` is set, the count is drawn once (before the slot writes) and
/// the slot-write + emitter body runs that many times — a loop the dispatcher
/// builds around a `call`.
pub struct Step {
    pub set: SlotPatch,
    pub emitter: Emitter,
    pub repeat: Option<Rand>,
}

/// A find-by-position overwrite of an already-built record.
///
/// Walks the buffer summing x-steps, finds the record covering `target_x`,
/// rewrites its x-step, and replaces its sprite/depth/y. (orig `0x12c26` plus a
/// half-emitter.)
pub struct Overwrite {
    pub target_x: u16,
    pub sprite: u16,
    pub depth: u16,
    pub y: u16,
}

/// A find-by-position insert that opens a one-record gap in the buffer.
///
/// Walks the buffer summing x-steps, finds the record covering `target_x`, and
/// splits its x-step into `target_x - before` / `after - target_x` to open the
/// gap. `records[0]` fills the inserted slot (keeping its split x-step); each
/// later entry overwrites the following record at x = 0. Mirrors the original's
/// `rep movsb` shift and the 5-record template / single-landmark fills. (orig L7
/// `0x12381` plus fill.)
pub struct Insert {
    pub target_x: u16,
    pub records: Vec<(u16, u16, u16)>,
}

/// A post-pass step: an in-place overwrite (L3) or a buffer-shift insert (L7).
pub enum PostOp {
    Overwrite(Overwrite),
    Insert(Insert),
}

/// The generated buffer's fixed capacity, `(0x2c3a - 0xd02) / 8` records.
///
/// An insert past it drops the tail, matching the original's bounded
/// `rep movsb`.
const BUFFER_CAPACITY: usize = (0x2c3a - 0xd02) / 8;

/// A fluent builder for a [`Step`].
///
/// Keeps the slot writes readable at the script call sites.
pub struct StepBuilder {
    set: SlotPatch,
    repeat: Option<Rand>,
}

/// Starts a [`StepBuilder`] with no slot writes and no repeat.
pub fn step() -> StepBuilder {
    StepBuilder {
        set: SlotPatch::default(),
        repeat: None,
    }
}

impl StepBuilder {
    /// Sets the x-start slot for this step.
    pub fn x_start(mut self, v: u16) -> Self {
        self.set.x_start = Some(v);
        self
    }

    /// Sets the x-step slot for this step.
    pub fn x_step(mut self, v: u16) -> Self {
        self.set.x_step = Some(v);
        self
    }

    /// Sets the sprite slot (read by the slot-driven emitters).
    pub fn sprite(mut self, v: u16) -> Self {
        self.set.sprite = Some(v);
        self
    }

    /// Sets the depth slot (read by the slot-driven emitters).
    pub fn depth(mut self, v: u16) -> Self {
        self.set.depth = Some(v);
        self
    }

    /// Sets the row-reset x slot (used by [`Grid`](Emitter::Grid) between rows).
    pub fn row_reset(mut self, v: u16) -> Self {
        self.set.row_reset = Some(v);
        self
    }

    /// Sets the row-y offset slot (added by an offset [`Grid`](Emitter::Grid)).
    pub fn row_y_offset(mut self, v: u16) -> Self {
        self.set.row_y_offset = Some(v);
        self
    }

    /// Repeats this step `rng(count) + base` times (a dispatcher-level loop).
    pub fn repeat(mut self, count: Rand) -> Self {
        self.repeat = Some(count);
        self
    }

    /// Finishes the builder, attaching `emitter` to produce the [`Step`].
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

/// Runs a script against a seeded PRNG and applies its post-pass.
///
/// Returns the generated [`Record`] buffer.
///
/// # Examples
///
/// ```
/// use openprototype::level::level_1;
/// use openprototype::level::prng::EngineRng;
/// use openprototype::level::slot::generate;
///
/// // Seed 0x3b95 reproduces LEVEL_1's validated GET-READY capture.
/// let records = generate(&level_1::script(), &[], &mut EngineRng::new(0x3b95));
/// assert_eq!(records.len(), 446);
/// ```
///
/// # Panics
///
/// Panics if a [`PostOp::Insert`] carries an empty `records` list.
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

        Emitter::Steps { count, y, fill } => {
            let n = draw(rng, *count);

            if matches!(fill, Fill::Slots) {
                rng.next(0xa); // vestigial row-y draw the slot routine never uses
            }

            for _ in 0..n {
                let (sprite, depth) = match fill {
                    Fill::Baked { sprite, depth } => (*sprite, *depth),
                    Fill::Slots => (slots.sprite, slots.depth),
                };

                out.push(Record {
                    x_step: slots.step_x(),
                    sprite,
                    depth,
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

        Emitter::Row {
            count,
            sprite,
            depth,
            y,
            style,
        } => {
            // Stepped draws the shared y after the count; Anchored draws it before.
            let (row_y, n) = match style {
                RowStyle::Stepped => {
                    let n = draw(rng, *count);
                    (draw(rng, *y), n)
                }
                RowStyle::Anchored { .. } => {
                    let row_y = draw(rng, *y);
                    (row_y, draw(rng, *count))
                }
            };

            let mut since_extra = 0u16;

            for _ in 0..n {
                let x_step = match style {
                    RowStyle::Stepped => slots.step_x(),
                    RowStyle::Anchored { x_base, .. } => x_base.wrapping_add(slots.consume_x()),
                };

                out.push(Record {
                    x_step,
                    sprite: *sprite,
                    depth: *depth,
                    y: row_y,
                });

                if let RowStyle::Anchored {
                    extra: Some(extra), ..
                } = style
                {
                    since_extra += 1;

                    if since_extra == 2 {
                        since_extra = 0;
                        out.push(Record {
                            x_step: 0,
                            sprite: extra.sprite,
                            depth: extra.depth,
                            y: draw(rng, extra.y),
                        });
                    }
                }
            }
        }

        Emitter::PairedRows {
            rows,
            sprite,
            depth,
            pair_when_one,
            pair_otherwise,
        } => {
            let n = draw(rng, *rows);
            // One rng(3) picks the shared y-pair for every row.
            let (y_lead, y_tail) = if rng.next(3) == 1 {
                *pair_when_one
            } else {
                *pair_otherwise
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

        Emitter::Fixed { repeat, cells } => {
            let n = match repeat {
                Some(r) => draw(rng, *r),
                None => 1,
            };

            for _ in 0..n {
                for cell in cells {
                    emit_cell(cell, slots, out);
                }
            }
        }

        Emitter::Scatter {
            count,
            x,
            sprite,
            depth,
            y,
        } => {
            let n = draw(rng, *count);

            for _ in 0..n {
                // x draws before y; the x-start is consumed during x.
                let x_step = draw(rng, *x).wrapping_add(slots.consume_x());
                let y = draw(rng, *y);
                out.push(Record {
                    x_step,
                    sprite: *sprite,
                    depth: *depth,
                    y,
                });
            }
        }

        Emitter::Choice { count, lo, hi } => {
            let n = draw(rng, *count);

            for _ in 0..n {
                let arm = if rng.next(5) > 1 { hi } else { lo };
                let x_step = draw(rng, arm.x).wrapping_add(slots.consume_x());
                let y = draw(rng, arm.y);
                out.push(Record {
                    x_step,
                    sprite: arm.sprite,
                    depth: arm.depth,
                    y,
                });
            }
        }
    }
}

fn emit_cell(cell: &Cell, slots: &mut Slots, out: &mut Vec<Record>) {
    let xs = match cell.x_start {
        XStart::None => 0,
        XStart::Consume => slots.consume_x(),
        XStart::Peek => slots.x_start,
        XStart::Step => slots.step_x(),
    };

    out.push(Record {
        x_step: cell.x_base.wrapping_add(xs),
        sprite: cell.sprite,
        depth: cell.depth,
        y: cell.y,
    });
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

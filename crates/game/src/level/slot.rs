//! The layout interpreter for all four generated levels (1, 3, 5, 7).
//!
//! Every generated level is a straight-line script of placement steps, each
//! running one emitter that appends 8-byte records to a growing buffer, sharing
//! the engine PRNG so the draw order is load-bearing. Most emitters bake their
//! sprite/health into the variant; a couple ([`Steps`](Emitter::Steps) with
//! [`Fill::Slots`] and [`Grid`](Emitter::Grid)) instead read them from engine
//! slots the dispatcher writes between steps and that persist across them. A
//! script may end with a post-pass that either overwrites a record in
//! place (LEVEL_3) or inserts one by splitting the covered record's delay
//! (LEVEL_7). This mirrors the disassembly, validated byte-for-byte against the
//! running game (each level's golden test reproduces its GET-READY capture). See
//! `reference/formats/level-layout.md`.

use super::prng::EngineRng;

/// One scheduled enemy or pickup spawn.
///
/// The consumer (the level's timer ISR + update loop) proves the field
/// semantics: `delay` is the spawn countdown in PIT ticks relative to the
/// previous record (the head record's field is decremented in place once per
/// tick; zero-delay records spawn the same frame). `sprite` is a cs-pointer to
/// the sprite descriptor block. `health` seeds the spawned entity's hit
/// points. `spawn_row` indexes the level's spawn-position table (`{x, y,
/// movement mode, movement arg}` rows); it is not a screen coordinate. The
/// generator's "x axis" (the `x_start`/`x_step` engine slots) is therefore the
/// spawn timeline in ticks. See `re/spawn-consumer.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record {
    pub delay: u16,
    pub sprite: u16,
    pub health: u16,
    pub spawn_row: u16,
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

/// One arm of a [`Choice`](Emitter::Choice).
///
/// A sprite/health with rng delay and spawn row.
#[derive(Clone, Copy)]
pub struct Arm {
    pub x: Rand,
    pub sprite: u16,
    pub health: u16,
    pub spawn_row: Rand,
}

/// The object a [`Row`](Emitter::Row) inserts after every second record.
///
/// It is always placed at delay 0 (same spawn tick as its row record).
#[derive(Clone, Copy)]
pub struct Extra {
    pub sprite: u16,
    pub health: u16,
    pub spawn_row: Rand,
}

/// Where a [`Steps`](Emitter::Steps) emitter gets each record's sprite/health.
#[derive(Clone, Copy)]
pub enum Fill {
    /// Hardcoded in the emitter.
    Baked { sprite: u16, health: u16 },
    /// From the engine slots the dispatcher wrote.
    ///
    /// The original's slot-reading routine also computes a row value it never
    /// uses, so this form burns one vestigial `rng(0xa)` draw before the
    /// loop (load-bearing for draw order).
    Slots,
}

/// How a [`Row`](Emitter::Row) lays out records and draws its shared row.
///
/// The two forms are different relinked routines, each with its own draw
/// order, so they are not interchangeable.
#[derive(Clone, Copy)]
pub enum RowStyle {
    /// Draws the shared row after the count; delay = x_start + x_step.
    Stepped,
    /// Draws the shared row before the count; delay = x_base + x_start.
    ///
    /// `extra` inserts an object (delay 0) after every second record.
    Anchored { x_base: u16, extra: Option<Extra> },
}

/// How a [`Cell`] uses the running x-start.
#[derive(Clone, Copy)]
pub enum XStart {
    /// Ignore it (delay = base).
    None,
    /// Add it, then zero it (the common consume).
    Consume,
    /// Add it but leave it set (the landmark leads that peek).
    Peek,
    /// Use x_start + x_step, then zero x_start (a stepped lead).
    Step,
}

/// One record of a [`Fixed`](Emitter::Fixed) block.
///
/// Always a constant spawn row; the `x_start` mode says how it uses the
/// running x-start.
#[derive(Clone, Copy)]
pub struct Cell {
    pub x_base: u16,
    pub x_start: XStart,
    pub sprite: u16,
    pub health: u16,
    pub spawn_row: u16,
}

/// The mutable engine slots the dispatcher writes and the emitters read.
#[derive(Default)]
struct Slots {
    x_start: u16,
    x_step: u16,
    sprite: u16,
    health: u16,
    row_reset: u16,
    spawn_row_offset: u16,
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
    pub health: Option<u16>,
    pub row_reset: Option<u16>,
    pub spawn_row_offset: Option<u16>,
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

        if let Some(v) = patch.health {
            self.health = v;
        }

        if let Some(v) = patch.row_reset {
            self.row_reset = v;
        }

        if let Some(v) = patch.spawn_row_offset {
            self.spawn_row_offset = v;
        }
    }

    /// delay = x_start + x_step, then consume x_start.
    ///
    /// The common per-record step.
    fn step_x(&mut self) -> u16 {
        let x = self.x_start.wrapping_add(self.x_step);
        self.x_start = 0;
        x
    }

    /// delay = x_start, then consume it.
    ///
    /// The landmark `Once` emitters; no step.
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
    /// Emits one record at delay x_start with a fixed sprite/health.
    ///
    /// (orig `0x121e7`/`12213`/`1223f`.)
    Once {
        sprite: u16,
        health: u16,
        spawn_row: Rand,
    },
    /// Emits `count` stepped records (delay = x_start + x_step), each with a
    /// random spawn row.
    ///
    /// `fill` is the sprite/health source: [`Baked`](Fill::Baked) hardcodes
    /// them, [`Slots`](Fill::Slots) reads them from the engine slots and burns
    /// one vestigial draw. (orig `0x1226b`/`122ab`; `Slots` form
    /// `0x123de`/`12434`.)
    Steps {
        count: Rand,
        spawn_row: Rand,
        fill: Fill,
    },
    /// Emits `outer` rows of `inner` records, with slot sprite/health.
    ///
    /// A spawn row is drawn once per grid row; a zero `inner` modulus uses a fixed
    /// inner count with no draw. After each row the x-start resets to
    /// `row_reset`. (orig `0x122eb`/`12367`.)
    Grid {
        outer: Rand,
        inner: Rand,
        spawn_row: Rand,
        spawn_row_uses_offset: bool,
    },
    /// Emits `count` records sharing one drawn spawn row, laid out per `style`.
    ///
    /// See [`RowStyle`] for the delay rule and the row draw order; sprite/health are
    /// hardcoded. (orig `0xfd82`/`0xff16`; anchored forms
    /// `0xe88a`/`0xe8d5`/`0xeab0`.)
    Row {
        count: Rand,
        sprite: u16,
        health: u16,
        spawn_row: Rand,
        style: RowStyle,
    },
    /// Emits `rows` rows of two records at a spawn-row pair picked by one
    /// `rng(3)`.
    ///
    /// Result `1` selects `pair_when_one`, anything else `pair_otherwise`. Per
    /// row the lead's delay = x_start + x_step and the tail's delay = 0;
    /// sprite/health are hardcoded. (orig `0xffe0`.)
    PairedRows {
        rows: Rand,
        sprite: u16,
        health: u16,
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
    /// Emits `count` records, each at a per-record drawn delay with a random
    /// spawn row.
    ///
    /// delay = `rng(x) + x_start` (consume, no x-step); the per-record delay
    /// draw is what sets it apart from [`Steps`](Emitter::Steps). (orig
    /// LEVEL_1's `0xe776` family.)
    Scatter {
        count: Rand,
        x: Rand,
        sprite: u16,
        health: u16,
        spawn_row: Rand,
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

/// A find-by-time overwrite of an already-built record.
///
/// Walks the buffer summing delays, finds the record covering `target_tick`,
/// rewrites its delay, and replaces its sprite/health/row. (orig `0x12c26`
/// plus a half-emitter.)
pub struct Overwrite {
    pub target_tick: u16,
    pub sprite: u16,
    pub health: u16,
    pub spawn_row: u16,
}

/// A find-by-time insert that opens a one-record gap in the buffer.
///
/// Walks the buffer summing delays, finds the record covering `target_tick`,
/// and splits its delay into `target_tick - before` / `after - target_tick` to
/// open the gap. `records[0]` fills the inserted slot (keeping its split
/// delay); each later entry overwrites the following record at delay 0.
/// Mirrors the original's
/// `rep movsb` shift and the 5-record template / single-landmark fills. (orig L7
/// `0x12381` plus fill.)
pub struct Insert {
    pub target_tick: u16,
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

    /// Sets the health slot (read by the slot-driven emitters).
    pub fn health(mut self, v: u16) -> Self {
        self.set.health = Some(v);
        self
    }

    /// Sets the row-reset x slot (used by [`Grid`](Emitter::Grid) between rows).
    pub fn row_reset(mut self, v: u16) -> Self {
        self.set.row_reset = Some(v);
        self
    }

    /// Sets the spawn-row offset slot (added by an offset
    /// [`Grid`](Emitter::Grid)).
    pub fn spawn_row_offset(mut self, v: u16) -> Self {
        self.set.spawn_row_offset = Some(v);
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
/// use openprototype::level::l1;
/// use openprototype::level::prng::EngineRng;
/// use openprototype::level::slot::generate;
///
/// // Seed 0x3b95 reproduces LEVEL_1's validated GET-READY capture.
/// let records = generate(&l1::script(), &[], &mut EngineRng::new(0x3b95));
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
        Emitter::Once {
            sprite,
            health,
            spawn_row,
        } => {
            out.push(Record {
                delay: slots.consume_x(),
                sprite: *sprite,
                health: *health,
                spawn_row: draw(rng, *spawn_row),
            });
        }

        Emitter::Steps {
            count,
            spawn_row,
            fill,
        } => {
            let n = draw(rng, *count);

            if matches!(fill, Fill::Slots) {
                rng.next(0xa); // vestigial row draw the slot routine never uses
            }

            for _ in 0..n {
                let (sprite, health) = match fill {
                    Fill::Baked { sprite, health } => (*sprite, *health),
                    Fill::Slots => (slots.sprite, slots.health),
                };

                out.push(Record {
                    delay: slots.step_x(),
                    sprite,
                    health,
                    spawn_row: draw(rng, *spawn_row),
                });
            }
        }

        Emitter::Grid {
            outer,
            inner,
            spawn_row,
            spawn_row_uses_offset,
        } => {
            let rows = draw(rng, *outer);

            for _ in 0..rows {
                // A zero inner modulus means a fixed inner count with no draw.
                let cols = if inner.modulus == 0 {
                    inner.base
                } else {
                    draw(rng, *inner)
                };

                let mut row = draw(rng, *spawn_row);

                if *spawn_row_uses_offset {
                    row = row.wrapping_add(slots.spawn_row_offset);
                }

                for _ in 0..cols {
                    out.push(Record {
                        delay: slots.step_x(),
                        sprite: slots.sprite,
                        health: slots.health,
                        spawn_row: row,
                    });
                }

                slots.x_start = slots.row_reset;
            }
        }

        Emitter::Row {
            count,
            sprite,
            health,
            spawn_row,
            style,
        } => {
            // Stepped draws the shared row after the count; Anchored draws it
            // before.
            let (row, n) = match style {
                RowStyle::Stepped => {
                    let n = draw(rng, *count);
                    (draw(rng, *spawn_row), n)
                }
                RowStyle::Anchored { .. } => {
                    let row = draw(rng, *spawn_row);
                    (row, draw(rng, *count))
                }
            };

            let mut since_extra = 0u16;

            for _ in 0..n {
                let delay = match style {
                    RowStyle::Stepped => slots.step_x(),
                    RowStyle::Anchored { x_base, .. } => x_base.wrapping_add(slots.consume_x()),
                };

                out.push(Record {
                    delay,
                    sprite: *sprite,
                    health: *health,
                    spawn_row: row,
                });

                if let RowStyle::Anchored {
                    extra: Some(extra), ..
                } = style
                {
                    since_extra += 1;

                    if since_extra == 2 {
                        since_extra = 0;
                        out.push(Record {
                            delay: 0,
                            sprite: extra.sprite,
                            health: extra.health,
                            spawn_row: draw(rng, extra.spawn_row),
                        });
                    }
                }
            }
        }

        Emitter::PairedRows {
            rows,
            sprite,
            health,
            pair_when_one,
            pair_otherwise,
        } => {
            let n = draw(rng, *rows);
            // One rng(3) picks the shared spawn-row pair for every row.
            let (lead_row, tail_row) = if rng.next(3) == 1 {
                *pair_when_one
            } else {
                *pair_otherwise
            };

            for _ in 0..n {
                out.push(Record {
                    delay: slots.step_x(),
                    sprite: *sprite,
                    health: *health,
                    spawn_row: lead_row,
                });
                out.push(Record {
                    delay: 0,
                    sprite: *sprite,
                    health: *health,
                    spawn_row: tail_row,
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
            health,
            spawn_row,
        } => {
            let n = draw(rng, *count);

            for _ in 0..n {
                // The delay draws before the row; the x-start is consumed
                // during the delay draw.
                let delay = draw(rng, *x).wrapping_add(slots.consume_x());
                let row = draw(rng, *spawn_row);
                out.push(Record {
                    delay,
                    sprite: *sprite,
                    health: *health,
                    spawn_row: row,
                });
            }
        }

        Emitter::Choice { count, lo, hi } => {
            let n = draw(rng, *count);

            for _ in 0..n {
                let arm = if rng.next(5) > 1 { hi } else { lo };
                let delay = draw(rng, arm.x).wrapping_add(slots.consume_x());
                let row = draw(rng, arm.spawn_row);
                out.push(Record {
                    delay,
                    sprite: arm.sprite,
                    health: arm.health,
                    spawn_row: row,
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
        delay: cell.x_base.wrapping_add(xs),
        sprite: cell.sprite,
        health: cell.health,
        spawn_row: cell.spawn_row,
    });
}

fn apply_overwrite(overwrite: &Overwrite, out: &mut [Record]) {
    let mut cumulative = 0u16;

    for record in out.iter_mut() {
        let before = cumulative;
        cumulative = cumulative.wrapping_add(record.delay);

        if cumulative > overwrite.target_tick {
            record.delay = overwrite.target_tick.wrapping_sub(before);
            record.sprite = overwrite.sprite;
            record.health = overwrite.health;
            record.spawn_row = overwrite.spawn_row;
            return;
        }
    }
}

fn apply_insert(insert: &Insert, out: &mut Vec<Record>) {
    let mut cumulative = 0u16;

    for i in 0..out.len() {
        let before = cumulative;
        cumulative = cumulative.wrapping_add(out[i].delay);

        if cumulative > insert.target_tick {
            // Split the covered record's delay across the new gap, then shift
            // the tail up one slot (dropping anything past the buffer
            // capacity).
            let new_delay = insert.target_tick.wrapping_sub(before);
            let remainder_delay = cumulative.wrapping_sub(insert.target_tick);

            out.insert(
                i,
                Record {
                    delay: new_delay,
                    sprite: 0,
                    health: 0,
                    spawn_row: 0,
                },
            );
            out[i + 1].delay = remainder_delay;
            out.truncate(BUFFER_CAPACITY);

            // First entry fills the inserted slot (keeping its split delay);
            // each later entry overwrites the following record at delay 0.
            let (sprite, health, spawn_row) = insert.records[0];
            out[i].sprite = sprite;
            out[i].health = health;
            out[i].spawn_row = spawn_row;

            for (offset, &(sprite, health, spawn_row)) in insert.records[1..].iter().enumerate() {
                out[i + 1 + offset] = Record {
                    delay: 0,
                    sprite,
                    health,
                    spawn_row,
                };
            }

            return;
        }
    }
}

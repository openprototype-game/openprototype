//! LEVEL_3 (WALD) layout data: the dispatcher script, emitter constants, depth
//! table, and find-by-position post-pass, transcribed from the disassembly and
//! validated byte-for-byte against the running game (seed `0x1a94` reproduces
//! the GET-READY capture). See `reference/formats/level-layout.md`.

use super::generator::rand;
use super::slot::{Emitter, Overwrite, Step, step};

/// Per-sprite-type depth (parallax layer), read by the original from a 9-entry
/// table at `cs:[dac5..dad5]`.
const DEPTHS: [u16; 9] = [0x82, 0x47e, 0x15e, 0x64, 0x8c, 0x50, 0xbe, 0xc8, 0x3a98];

/// The foreground depth the landmark `Once` emitters hardcode (`0xfa`).
const FOREGROUND: u16 = 0xfa;

// Emitter builders, named for the original routine each transcribes. The
// dispatcher supplies the per-step counts (and grid row/column spreads).

fn once(sprite: u16, y_base: u16) -> Emitter {
    Emitter::Once {
        sprite,
        depth: FOREGROUND,
        y: rand(3, y_base),
    }
}

fn single_56b6(count_mod: u16, count_base: u16) -> Emitter {
    Emitter::Single {
        count: rand(count_mod, count_base),
        sprite: 0x56b6,
        depth: DEPTHS[2],
        y: rand(7, 0x57),
    }
}

fn single_5928(count_mod: u16, count_base: u16) -> Emitter {
    Emitter::Single {
        count: rand(count_mod, count_base),
        sprite: 0x5928,
        depth: DEPTHS[6],
        y: rand(7, 0x6a),
    }
}

fn s123de(count_mod: u16, count_base: u16) -> Emitter {
    Emitter::SlotSingle {
        count: rand(count_mod, count_base),
        y: rand(0xe, 9),
    }
}

fn s12434(count_mod: u16, count_base: u16) -> Emitter {
    Emitter::SlotSingle {
        count: rand(count_mod, count_base),
        y: rand(0xe, 0x17),
    }
}

// Grid args mirror the dispatcher registers: inner count = `rng(ax) + bx`, outer
// row count = `rng(dx) + cx`.

fn grid122eb(ax: u16, bx: u16, cx: u16, dx: u16) -> Emitter {
    Emitter::Grid {
        outer: rand(dx, cx),
        inner: rand(ax, bx),
        row_y: rand(0xa, 0x25),
        row_y_uses_offset: true,
    }
}

fn grid12367(ax: u16, bx: u16, cx: u16, dx: u16) -> Emitter {
    Emitter::Grid {
        outer: rand(dx, cx),
        inner: rand(ax, bx),
        row_y: rand(0xa, 0x39),
        row_y_uses_offset: false,
    }
}

fn fixed1248a(count: u16) -> Emitter {
    Emitter::FixedRun {
        sprite: 0x54d6,
        depth: DEPTHS[1],
        y_lead: 0x5e,
        mid: (0x32, 0x5f),
        run: (0xf, 0x60),
        count,
    }
}

/// LEVEL_3's 38-step append script, in order. Steps set only the slots the
/// original writes; the rest carry over.
pub fn script() -> Vec<Step> {
    vec![
        step()
            .x_start(0)
            .row_reset(0x96)
            .x_step(0x28)
            .sprite(0x5818)
            .depth(DEPTHS[4])
            .emit(s123de(0xb, 0xf)),
        step()
            .x_start(0x96)
            .x_step(0xf)
            .row_reset(0x96)
            .sprite(0x57e2)
            .depth(DEPTHS[3])
            .row_y_offset(0x28)
            .emit(grid122eb(6, 0xa, 2, 3)),
        step().x_start(5).emit(once(0x510c, 0)),
        step()
            .x_start(0xc8)
            .x_step(0x46)
            .emit(single_56b6(0xb, 0xa)),
        step()
            .x_start(0x64)
            .x_step(0xf)
            .row_reset(0xa0)
            .sprite(0x57e2)
            .depth(DEPTHS[3])
            .row_y_offset(0x28)
            .emit(grid122eb(6, 0xa, 1, 2)),
        step()
            .x_start(0x96)
            .x_step(0xf)
            .row_reset(0xc8)
            .sprite(0x583e)
            .depth(DEPTHS[4])
            .row_y_offset(0)
            .emit(grid12367(8, 7, 3, 2)),
        step().x_start(5).emit(once(0x5172, 3)),
        step()
            .x_start(0x64)
            .x_step(0xa)
            .sprite(0x5864)
            .depth(DEPTHS[4])
            .emit(s123de(0xb, 0xf)),
        step()
            .x_start(0x64)
            .x_step(0xf)
            .row_reset(0x64)
            .sprite(0x57e2)
            .depth(DEPTHS[3])
            .row_y_offset(0x28)
            .emit(grid122eb(6, 5, 2, 2)),
        step().x_start(0xbe).x_step(0x46).emit(single_56b6(6, 0xa)),
        step()
            .x_start(0xaa)
            .x_step(0x14)
            .sprite(0x588a)
            .depth(DEPTHS[5])
            .emit(s12434(6, 0xa)),
        step().x_start(0x64).emit(fixed1248a(2)),
        step()
            .x_start(0xaa)
            .x_step(0xf)
            .sprite(0x54b0)
            .depth(DEPTHS[0])
            .emit(s123de(6, 0xf)),
        step()
            .x_start(0x96)
            .x_step(0x14)
            .sprite(0x583e)
            .depth(DEPTHS[4])
            .emit(s12434(6, 0xa)),
        step().x_start(0x8c).x_step(0x46).emit(single_56b6(6, 5)),
        step().x_start(0xbe).x_step(0x32).emit(single_5928(6, 0xa)),
        step()
            .x_start(0xc8)
            .x_step(0x14)
            .sprite(0x583e)
            .depth(DEPTHS[4])
            .emit(s12434(6, 0xf)),
        step()
            .x_start(0xaa)
            .x_step(0xf)
            .sprite(0x5864)
            .depth(DEPTHS[5])
            .emit(s123de(6, 0xf)),
        step().x_start(0x8c).x_step(0x46).emit(single_56b6(6, 5)),
        step()
            .x_start(0x64)
            .x_step(0x19)
            .row_reset(0)
            .sprite(0x5864)
            .depth(DEPTHS[5])
            .row_y_offset(0)
            .emit(grid122eb(0, 1, 5, 4)),
        step()
            .x_start(0x64)
            .x_step(0x1e)
            .row_reset(0)
            .sprite(0x588a)
            .depth(DEPTHS[5])
            .row_y_offset(0)
            .emit(grid12367(0, 1, 5, 4)),
        step().x_start(0x64).emit(fixed1248a(5)),
        step().x_start(0x5a).x_step(0x46).emit(single_56b6(6, 0xa)),
        step()
            .x_start(0xaa)
            .x_step(0x14)
            .sprite(0x54b0)
            .depth(DEPTHS[0])
            .row_y_offset(0)
            .emit(s123de(6, 0xf)),
        step()
            .x_start(0xaa)
            .x_step(0xf)
            .sprite(0x583e)
            .depth(DEPTHS[4])
            .emit(s12434(6, 0xf)),
        step().x_start(0xbe).x_step(0x32).emit(single_5928(6, 0xa)),
        step()
            .x_start(0x64)
            .x_step(0x19)
            .row_reset(0)
            .sprite(0x56b6)
            .depth(DEPTHS[2])
            .row_y_offset(0xa)
            .emit(grid122eb(0, 1, 5, 4)),
        step().row_y_offset(0).x_start(5).emit(once(0x510c, 0)),
        step()
            .x_start(0xaa)
            .x_step(0x14)
            .sprite(0x54b0)
            .depth(DEPTHS[0])
            .row_y_offset(0)
            .emit(s123de(6, 0xf)),
        step()
            .x_start(0x64)
            .x_step(0x1e)
            .row_reset(0)
            .sprite(0x588a)
            .depth(DEPTHS[5])
            .row_y_offset(0)
            .emit(grid12367(0, 1, 5, 4)),
        step()
            .x_start(0x96)
            .x_step(0xf)
            .row_reset(0x96)
            .sprite(0x57e2)
            .depth(DEPTHS[3])
            .row_y_offset(0x28)
            .emit(grid122eb(6, 0xa, 2, 3)),
        step()
            .x_start(0x64)
            .x_step(0x19)
            .row_reset(0)
            .sprite(0x56b6)
            .depth(DEPTHS[2])
            .row_y_offset(0xa)
            .emit(grid122eb(0, 1, 5, 4)),
        step().row_y_offset(0).x_start(5).emit(once(0x52ae, 6)),
        step()
            .x_start(0xaa)
            .x_step(0xf)
            .sprite(0x54b0)
            .depth(DEPTHS[0])
            .emit(s123de(6, 0xa)),
        step().x_start(0xbe).x_step(0x32).emit(single_5928(6, 0xa)),
        step()
            .x_start(0x64)
            .x_step(0xa)
            .sprite(0x5864)
            .depth(DEPTHS[4])
            .emit(s123de(0xb, 0xf)),
        step()
            .row_reset(0x96)
            .x_step(0x28)
            .sprite(0x5818)
            .depth(DEPTHS[4])
            .emit(s123de(0xb, 0xf)),
        step()
            .x_start(0xaa)
            .x_step(0x14)
            .sprite(0x588a)
            .depth(DEPTHS[5])
            .emit(s12434(6, 0xa)),
    ]
}

fn overwrite(target_x: u16, sprite: u16, depth: u16, y: u16) -> Overwrite {
    Overwrite {
        target_x,
        sprite,
        depth,
        y,
    }
}

/// LEVEL_3's find-by-position post-pass: 6 `0x58b0` landmarks near the start, 21
/// `0x5ac4` markers across the mid scroll, and a single `0x5c20` at the far end.
pub fn post_pass() -> Vec<Overwrite> {
    let mut out = Vec::new();

    for (target_x, y) in [
        (0x34, 0x68),
        (0x80, 0x66),
        (0x90, 0x65),
        (0x1c0, 0x66),
        (0x2e4, 0x67),
        (0x37e, 0x69),
    ] {
        out.push(overwrite(target_x, 0x58b0, DEPTHS[6], y));
    }

    for (target_x, y) in [
        (0x21c0, 0x62),
        (0x2240, 0x62),
        (0x22a0, 0x63),
        (0x2300, 0x62),
        (0x2380, 0x63),
        (0x2400, 0x62),
        (0x2460, 0x62),
        (0x2550, 0x63),
        (0x3190, 0x62),
        (0x3220, 0x63),
        (0x32a0, 0x63),
        (0x3360, 0x62),
        (0x3440, 0x63),
        (0x3500, 0x62),
        (0x3590, 0x62),
        (0x3650, 0x63),
        (0x3700, 0x62),
        (0x3790, 0x63),
        (0x3840, 0x63),
        (0x3900, 0x62),
        (0x39a0, 0x63),
    ] {
        out.push(overwrite(target_x, 0x5ac4, DEPTHS[7], y));
    }

    out.push(overwrite(0x4300, 0x5c20, DEPTHS[8], 0x61));
    out
}

#[cfg(test)]
mod tests {
    use super::{post_pass, script};
    use crate::level::generator::Record;
    use crate::level::prng::EngineRng;
    use crate::level::slot::generate;

    fn record(x_step: u16, sprite: u16, depth: u16, y: u16) -> Record {
        Record {
            x_step,
            sprite,
            depth,
            y,
        }
    }

    #[test]
    fn reproduces_the_validated_capture() {
        // Seed 0x1a94 is the wall-clock seed of the captured GET-READY state.
        let records = generate(&script(), &post_pass(), &mut EngineRng::new(0x1a94));

        assert_eq!(records.len(), 508);

        // Spot-checks across the emitter variety and the post-pass overwrites.
        assert_eq!(records[0], record(0x28, 0x5818, 0x8c, 0x10)); // SlotSingle lead
        assert_eq!(records[1], record(0xc, 0x58b0, 0xbe, 0x68)); // 0x58b0 overwrite
        assert_eq!(records[50], record(0xf, 0x57e2, 0x64, 0x53)); // Grid row
        assert_eq!(records[294], record(0xf, 0x54d6, 0x47e, 0x60)); // FixedRun tail
        assert_eq!(records[309], record(0xbe, 0x54b0, 0x82, 0x14)); // SlotSingle lead
        assert_eq!(records[345], record(0xf, 0x583e, 0x8c, 0x1f));
        assert_eq!(records[507], record(0x14, 0x588a, 0x50, 0x23)); // last record

        // The post-pass stamps a fixed number of each landmark sprite.
        let count = |sprite| records.iter().filter(|r| r.sprite == sprite).count();
        assert_eq!(count(0x58b0), 6);
        assert_eq!(count(0x5ac4), 21);
        assert_eq!(count(0x5c20), 1);
    }

    #[test]
    fn is_deterministic_for_a_seed() {
        let a = generate(&script(), &post_pass(), &mut EngineRng::new(0x1234));
        let b = generate(&script(), &post_pass(), &mut EngineRng::new(0x1234));
        assert_eq!(a, b);
    }
}

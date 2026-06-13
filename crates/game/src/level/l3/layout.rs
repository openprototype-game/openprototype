//! LEVEL_3 (WALD) layout data: script, constants, health table, and post-pass.
//!
//! Transcribed from the disassembly and validated byte-for-byte against the
//! running game (seed `0x1a94` reproduces the GET-READY capture). See
//! `reference/formats/level-layout.md`.

use super::{
    BAT, BAT_HANGING, BLUE_BEETLE, BOSS, EXTRA_LIFE, INVINCIBILITY, LURKER, MOSQUITO, PTERODACTYL,
    RED_BEETLE, SMART_BOMB, WASP_L, WASP_R, WASPLING_L, WASPLING_R,
};
use crate::level::slot::{Cell, Emitter, Fill, Overwrite, PostOp, Step, XStart, rand, step};

/// Per-sprite-type spawn health, read by the original from a 9-entry table
/// at `cs:[dac5..dad5]`.
const HEALTHS: [u16; 9] = [0x82, 0x47e, 0x15e, 0x64, 0x8c, 0x50, 0xbe, 0xc8, 0x3a98];

/// The health the landmark pickup `Once` emitters hardcode (`0xfa` = 250).
const PICKUP_HEALTH: u16 = 0xfa;

// Emitter builders: enemy-specific ones bake their sprite/health; the generic
// slot/grid builders take the sprite from the step. The dispatcher supplies
// the per-step counts (and grid row/column spreads). Original code addresses:
// slot_steps 123de, slot_steps_2 12434, grid_offset 122eb, grid_plain 12367,
// red_beetle_block 1248a.

fn once(sprite: u16, row_base: u16) -> Emitter {
    Emitter::Once {
        sprite,
        health: PICKUP_HEALTH,
        spawn_row: rand(3, row_base),
    }
}

fn pterodactyl_steps(count_mod: u16, count_base: u16) -> Emitter {
    Emitter::Steps {
        count: rand(count_mod, count_base),
        spawn_row: rand(7, 0x57),
        fill: Fill::Baked {
            sprite: PTERODACTYL,
            health: HEALTHS[2],
        },
    }
}

fn bat_steps(count_mod: u16, count_base: u16) -> Emitter {
    Emitter::Steps {
        count: rand(count_mod, count_base),
        spawn_row: rand(7, 0x6a),
        fill: Fill::Baked {
            sprite: BAT,
            health: HEALTHS[6],
        },
    }
}

fn slot_steps(count_mod: u16, count_base: u16) -> Emitter {
    Emitter::Steps {
        count: rand(count_mod, count_base),
        spawn_row: rand(0xe, 9),
        fill: Fill::Slots,
    }
}

fn slot_steps_2(count_mod: u16, count_base: u16) -> Emitter {
    Emitter::Steps {
        count: rand(count_mod, count_base),
        spawn_row: rand(0xe, 0x17),
        fill: Fill::Slots,
    }
}

// Grid args mirror the dispatcher registers: inner count = `rng(ax) + bx`, outer
// row count = `rng(dx) + cx`.

fn grid_offset(ax: u16, bx: u16, cx: u16, dx: u16) -> Emitter {
    Emitter::Grid {
        outer: rand(dx, cx),
        inner: rand(ax, bx),
        spawn_row: rand(0xa, 0x25),
        spawn_row_uses_offset: true,
    }
}

fn grid_plain(ax: u16, bx: u16, cx: u16, dx: u16) -> Emitter {
    Emitter::Grid {
        outer: rand(dx, cx),
        inner: rand(ax, bx),
        spawn_row: rand(0xa, 0x39),
        spawn_row_uses_offset: false,
    }
}

fn red_beetle_block(count: u16) -> Emitter {
    let sprite = RED_BEETLE;
    let health = HEALTHS[1];

    // A stepped lead, a mid record, then `count` identical trailing records.
    let mut cells = vec![
        Cell {
            x_base: 0,
            x_start: XStart::Step,
            sprite,
            health,
            spawn_row: 0x5e,
        },
        Cell {
            x_base: 0x32,
            x_start: XStart::None,
            sprite,
            health,
            spawn_row: 0x5f,
        },
    ];

    for _ in 0..count {
        cells.push(Cell {
            x_base: 0xf,
            x_start: XStart::None,
            sprite,
            health,
            spawn_row: 0x60,
        });
    }

    Emitter::Fixed {
        repeat: None,
        cells,
    }
}

/// Returns LEVEL_3's 38-step append script, in order.
///
/// Steps set only the slots the original writes; the rest carry over.
pub fn script() -> Vec<Step> {
    vec![
        step()
            .x_start(0)
            .row_reset(0x96)
            .x_step(0x28)
            .sprite(WASP_L)
            .health(HEALTHS[4])
            .emit(slot_steps(0xb, 0xf)),
        step()
            .x_start(0x96)
            .x_step(0xf)
            .row_reset(0x96)
            .sprite(BLUE_BEETLE)
            .health(HEALTHS[3])
            .spawn_row_offset(0x28)
            .emit(grid_offset(6, 0xa, 2, 3)),
        step().x_start(5).emit(once(SMART_BOMB, 0)),
        step()
            .x_start(0xc8)
            .x_step(0x46)
            .emit(pterodactyl_steps(0xb, 0xa)),
        step()
            .x_start(0x64)
            .x_step(0xf)
            .row_reset(0xa0)
            .sprite(BLUE_BEETLE)
            .health(HEALTHS[3])
            .spawn_row_offset(0x28)
            .emit(grid_offset(6, 0xa, 1, 2)),
        step()
            .x_start(0x96)
            .x_step(0xf)
            .row_reset(0xc8)
            .sprite(WASP_R)
            .health(HEALTHS[4])
            .spawn_row_offset(0)
            .emit(grid_plain(8, 7, 3, 2)),
        step().x_start(5).emit(once(INVINCIBILITY, 3)),
        step()
            .x_start(0x64)
            .x_step(0xa)
            .sprite(WASPLING_L)
            .health(HEALTHS[4])
            .emit(slot_steps(0xb, 0xf)),
        step()
            .x_start(0x64)
            .x_step(0xf)
            .row_reset(0x64)
            .sprite(BLUE_BEETLE)
            .health(HEALTHS[3])
            .spawn_row_offset(0x28)
            .emit(grid_offset(6, 5, 2, 2)),
        step()
            .x_start(0xbe)
            .x_step(0x46)
            .emit(pterodactyl_steps(6, 0xa)),
        step()
            .x_start(0xaa)
            .x_step(0x14)
            .sprite(WASPLING_R)
            .health(HEALTHS[5])
            .emit(slot_steps_2(6, 0xa)),
        step().x_start(0x64).emit(red_beetle_block(2)),
        step()
            .x_start(0xaa)
            .x_step(0xf)
            .sprite(MOSQUITO)
            .health(HEALTHS[0])
            .emit(slot_steps(6, 0xa)),
        step()
            .x_start(0x96)
            .x_step(0x14)
            .sprite(WASP_R)
            .health(HEALTHS[4])
            .emit(slot_steps_2(6, 0xa)),
        step()
            .x_start(0x8c)
            .x_step(0x46)
            .emit(pterodactyl_steps(6, 5)),
        step().x_start(0xbe).x_step(0x32).emit(bat_steps(6, 0xa)),
        step()
            .x_start(0xc8)
            .x_step(0x14)
            .sprite(WASP_R)
            .health(HEALTHS[4])
            .emit(slot_steps_2(6, 0xf)),
        step()
            .x_start(0xaa)
            .x_step(0xf)
            .sprite(WASPLING_L)
            .health(HEALTHS[5])
            .emit(slot_steps(6, 0xf)),
        step()
            .x_start(0x8c)
            .x_step(0x46)
            .emit(pterodactyl_steps(6, 5)),
        step()
            .x_start(0x64)
            .x_step(0x19)
            .row_reset(0)
            .sprite(WASPLING_L)
            .health(HEALTHS[5])
            .spawn_row_offset(0)
            .emit(grid_offset(0, 1, 5, 4)),
        step()
            .x_start(0x64)
            .x_step(0x1e)
            .row_reset(0)
            .sprite(WASPLING_R)
            .health(HEALTHS[5])
            .spawn_row_offset(0)
            .emit(grid_plain(0, 1, 5, 4)),
        step().x_start(0x64).emit(red_beetle_block(5)),
        step()
            .x_start(0x5a)
            .x_step(0x46)
            .emit(pterodactyl_steps(6, 0xa)),
        step()
            .x_start(0xaa)
            .x_step(0x14)
            .sprite(MOSQUITO)
            .health(HEALTHS[0])
            .spawn_row_offset(0)
            .emit(slot_steps(6, 0xf)),
        step()
            .x_start(0xaa)
            .x_step(0xf)
            .sprite(WASP_R)
            .health(HEALTHS[4])
            .emit(slot_steps_2(6, 0xf)),
        step().x_start(0xbe).x_step(0x32).emit(bat_steps(6, 0xa)),
        step()
            .x_start(0x64)
            .x_step(0x19)
            .row_reset(0)
            .sprite(PTERODACTYL)
            .health(HEALTHS[2])
            .spawn_row_offset(0xa)
            .emit(grid_offset(0, 1, 5, 4)),
        step()
            .spawn_row_offset(0)
            .x_start(5)
            .emit(once(SMART_BOMB, 0)),
        step()
            .x_start(0xaa)
            .x_step(0x14)
            .sprite(MOSQUITO)
            .health(HEALTHS[0])
            .spawn_row_offset(0)
            .emit(slot_steps(6, 0xf)),
        step()
            .x_start(0x64)
            .x_step(0x1e)
            .row_reset(0)
            .sprite(WASPLING_R)
            .health(HEALTHS[5])
            .spawn_row_offset(0)
            .emit(grid_plain(0, 1, 5, 4)),
        step()
            .x_start(0x96)
            .x_step(0xf)
            .row_reset(0x96)
            .sprite(BLUE_BEETLE)
            .health(HEALTHS[3])
            .spawn_row_offset(0x28)
            .emit(grid_offset(6, 0xa, 2, 3)),
        step()
            .x_start(0x64)
            .x_step(0x19)
            .row_reset(0)
            .sprite(PTERODACTYL)
            .health(HEALTHS[2])
            .spawn_row_offset(0xa)
            .emit(grid_offset(0, 1, 5, 4)),
        step()
            .spawn_row_offset(0)
            .x_start(5)
            .emit(once(EXTRA_LIFE, 6)),
        step()
            .x_start(0xaa)
            .x_step(0xf)
            .sprite(MOSQUITO)
            .health(HEALTHS[0])
            .emit(slot_steps(6, 0xa)),
        step().x_start(0xbe).x_step(0x32).emit(bat_steps(6, 0xa)),
        step()
            .x_start(0x64)
            .x_step(0xa)
            .sprite(WASPLING_L)
            .health(HEALTHS[4])
            .emit(slot_steps(0xb, 0xf)),
        step()
            .row_reset(0x96)
            .x_step(0x28)
            .sprite(WASP_L)
            .health(HEALTHS[4])
            .emit(slot_steps(0xb, 0xf)),
        step()
            .x_start(0xaa)
            .x_step(0x14)
            .sprite(WASPLING_R)
            .health(HEALTHS[5])
            .emit(slot_steps_2(6, 0xa)),
    ]
}

fn overwrite(target_tick: u16, sprite: u16, health: u16, spawn_row: u16) -> PostOp {
    PostOp::Overwrite(Overwrite {
        target_tick,
        sprite,
        health,
        spawn_row,
    })
}

/// Returns LEVEL_3's find-by-position overwrite post-pass.
///
/// Stamps 6 `0x58b0` landmarks near the start, 21 `0x5ac4` markers across the
/// mid scroll, and a single `0x5c20` at the far end.
pub fn post_pass() -> Vec<PostOp> {
    let mut out = Vec::new();

    for (target_tick, row) in [
        (0x34, 0x68),
        (0x80, 0x66),
        (0x90, 0x65),
        (0x1c0, 0x66),
        (0x2e4, 0x67),
        (0x37e, 0x69),
    ] {
        out.push(overwrite(target_tick, BAT_HANGING, HEALTHS[6], row));
    }

    for (target_tick, row) in [
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
        out.push(overwrite(target_tick, LURKER, HEALTHS[7], row));
    }

    out.push(overwrite(0x4300, BOSS, HEALTHS[8], 0x61));
    out
}

#[cfg(test)]
mod tests {
    use super::{post_pass, script};
    use crate::level::golden_hash;
    use crate::level::prng::EngineRng;
    use crate::level::slot::generate;

    /// FNV-1a over the full 508-record buffer (post-pass included) for the
    /// validated seed. Locks the layout byte-for-byte against refactors;
    /// regenerate and re-verify against the capture if it ever changes.
    const GOLDEN: &str = "79e4215aa84d2327";

    #[test]
    fn reproduces_the_validated_capture() {
        // Seed 0x1a94 is the wall-clock seed of the captured GET-READY state.
        let records = generate(&script(), &post_pass(), &mut EngineRng::new(0x1a94));

        assert_eq!(records.len(), 508);
        assert_eq!(golden_hash(&records), GOLDEN);
    }

    #[test]
    fn is_deterministic_for_a_seed() {
        let a = generate(&script(), &post_pass(), &mut EngineRng::new(0x1234));
        let b = generate(&script(), &post_pass(), &mut EngineRng::new(0x1234));
        assert_eq!(a, b);
    }
}

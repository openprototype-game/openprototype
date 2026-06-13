//! LEVEL_3's 56 mode-0 enemy AI functions, transcribed from the disassembly
//! (`re/l3-ai-functions.md`; pointer table at file `0x11066`).
//!
//! Same engine as L1 with a different AI set. The big structural difference:
//! most L3 sprite families are runs of 0x1e-byte full descriptors per frame
//! (each frame carries its own hitboxes), so the functions re-copy the
//! current frame's hitboxes every sub-step. Only the pickups and the popper
//! family keep L1-style 8-byte cycle frames.
//!
//! Positions are 12.4 fixed point; data tables are read from the WAD image
//! at their file offsets.

use crate::level::prng::EngineRng;
use crate::spawns::{AiSounds, BossExplosionSound, Effect, Entity, Shot, descriptor_hitboxes};

/// LEVEL_3's cs-pointer to file-offset base.
const CS_BASE: usize = 0x4710;

/// The boss volley's sample (lgegshot, the per-level slot 8).
const SLOT_VOLLEY: usize = 8;

/// Per-step context the AI functions read and write besides the entity.
pub(crate) struct AiContext<'a> {
    pub wad: &'a [u8],
    pub rng: &'a mut EngineRng,
    /// Player position in pixels (camera-inclusive buffer coordinates).
    pub player_x: i32,
    pub player_y: i32,
    pub shots: &'a mut Vec<Shot>,
    pub effects: &'a mut Vec<Effect>,
    pub boss: &'a mut BossState,
    /// The boss/orbiter gate counter (`cs:0x394e`).
    pub gate: &'a mut u8,
    /// A boss explosion burst fired this step.
    pub boss_explosion: &'a mut Option<BossExplosionSound>,
    /// Sample slots the AI triggered this step (event channel).
    pub sounds: &'a mut AiSounds,
    /// Whether the firing weapon is the plasma (`cs:0xcb5 == 3`); it bypasses
    /// the orbiters' attack-animation proximity gate.
    pub firing_plasma: bool,
}

/// The boss's engine globals (`cs:0xcd7..0xcf5`); one boss runs at a time,
/// so the original keeps these outside the entity.
pub(crate) struct BossState {
    /// Y-bob phase (`cs:0xcd9`, wrap 0x28, byte-indexed deltas).
    bob_phase: usize,
    /// The boss script tick (`cs:0xcd7`); the pattern loop resets it to
    /// 0x1a5 to stay past the intro script.
    tick: u16,
    /// Hover frame-delta index (`cs:0xcdb`, wrap 0x50).
    frame_index: usize,
    /// Creeping hover anchor x and the home anchor x (`cs:0xcdd`/`0xcdf`).
    creep_x: i32,
    home_x: i32,
    /// The lunge's end x (`cs:0xce1`), the wave's center.
    lunge_end_x: i32,
    /// Sine index (`cs:0xce3`, wrap 0xf0).
    sine_index: usize,
    /// Hover counter (`cs:0xce5`) and pattern counter (`cs:0xce7`).
    hover_count: u16,
    pattern_count: u16,
    /// The lunge's every-2nd-call divider (`cs:0xce9`).
    divider: u8,
    /// Volley timer (`cs:0xcef`, file-image init 0x168, reload 0x28).
    fire_timer: i32,
    /// Explosion-burst timer (`cs:0xcf1`, init 0x14) and offsets.
    explosion_timer: i32,
    explosion_dx: i32,
    explosion_dy: i32,
}

impl Default for BossState {
    fn default() -> Self {
        Self {
            bob_phase: 0,
            tick: 0,
            frame_index: 0,
            creep_x: 0,
            home_x: 0,
            lunge_end_x: 0,
            sine_index: 0,
            hover_count: 0,
            pattern_count: 0,
            divider: 0,
            fire_timer: 0x168,
            explosion_timer: 0x14,
            explosion_dx: 0,
            explosion_dy: 0,
        }
    }
}

/// Reads an i16 word from the WAD image.
fn word(wad: &[u8], at: usize) -> i32 {
    if wad.len() < at + 2 {
        return 0;
    }

    i32::from(i16::from_le_bytes([wad[at], wad[at + 1]]))
}

/// Re-copies the current frame's hitboxes (file `0x100c3`); the 0x1e-byte
/// frame families call this every sub-step so the boxes track the pose.
fn copy_hitbox(entity: &mut Entity, wad: &[u8]) {
    entity.hitboxes = descriptor_hitboxes(wad, CS_BASE, entity.sprite);
}

/// The left-moving path segments, in arg order (funcs 7-16, 17-26, 38-47).
const LEFT_SEGMENTS: [usize; 10] = [
    0x38, 0x5e, 0x84, 0xaa, 0xdc, 0x102, 0x128, 0x15a, 0x18c, 0x1b2,
];

/// The right-moving path segments (funcs 27-36; their spawns enter at x -30).
const RIGHT_SEGMENTS: [usize; 10] = [
    0x1e4, 0x223, 0x255, 0x287, 0x2b9, 0x2eb, 0x325, 0x35a, 0x399, 0x3d3,
];

/// A path segment's file offset (`segment * 16 + 0x200`).
fn segment_base(segment: usize) -> usize {
    segment * 16 + 0x200
}

/// The orbiter wave table (file `0x103fd`): 0x78 words, byte-indexed with a
/// 0xf0 wrap.
const ORBITER_WAVE: usize = 0x103fd;

/// Runs AI function `arg` for one sub-step.
pub(crate) fn step(entity: &mut Entity, ctx: &mut AiContext) {
    match entity.arg {
        0 => pickup(entity, 0x51e8, 0x5246),
        1 => pickup(entity, 0x510c, 0x516a),
        2 => pickup(entity, 0x5172, 0x51e0),
        3 => pickup(entity, 0x52ae, 0x52f4),
        4 => {
            entity.x -= 0x30;
            flap(entity);
        }
        5 => {
            entity.x += 0x30;
            flap(entity);
        }
        6 => {
            entity.x -= 0x18;
            walker_anim(entity);
            copy_hitbox(entity, ctx.wad);
        }
        7..=16 => {
            path_step(entity, ctx.wad, LEFT_SEGMENTS[usize::from(entity.arg) - 7]);
            flap(entity);
        }
        17..=26 => {
            path_step(entity, ctx.wad, LEFT_SEGMENTS[usize::from(entity.arg) - 17]);
            entity.tick += 1;
            walker_anim(entity);
            copy_hitbox(entity, ctx.wad);
        }
        27..=36 => {
            path_step(
                entity,
                ctx.wad,
                RIGHT_SEGMENTS[usize::from(entity.arg) - 27],
            );
            flap(entity);
        }
        37 => {
            entity.x -= 0x28;
            popper_anim(entity);
        }
        38..=47 => {
            path_step(entity, ctx.wad, LEFT_SEGMENTS[usize::from(entity.arg) - 38]);
            popper_anim(entity);
        }
        48 => orbiter(entity, ctx, 0xc8, OrbitShape::Single),
        49 => orbiter(entity, ctx, 0xb4, OrbitShape::Double),
        50 => orbiter(entity, ctx, 0x50, OrbitShape::Wide),
        51 => slow_drifter(entity, ctx.wad),
        52 => leaper(entity, ctx, LEAP_STEEP),
        53 => leaper(entity, ctx, LEAP_GENTLE),
        54 => boss(entity, ctx),
        55 => {
            entity.x -= 0x18;
            entity.anim += 1;

            if entity.anim == 4 {
                entity.anim = 0;
                entity.sprite = if entity.sprite == 0x5a36 {
                    0x5928
                } else {
                    entity.sprite + 0x1e
                };
            }

            copy_hitbox(entity, ctx.wad);
        }
        _ => {}
    }
}

/// Pickup drifters (funcs 0-3): drift left, 8-byte cycle frames after a
/// 0x1e-byte rest descriptor.
fn pickup(entity: &mut Entity, rest: u16, last: u16) {
    entity.x -= 0x14;
    entity.tick += 1;
    entity.anim += 1;

    if entity.anim == 4 {
        entity.anim = 0;
        entity.sprite = if entity.sprite == rest {
            rest + 0x1e
        } else if entity.sprite == last {
            rest
        } else {
            entity.sprite + 8
        };
    }
}

/// The two-frame flap (file `0x10097`): every 3rd call the sprite toggles
/// between the rest descriptor and rest + 0x1e.
fn flap(entity: &mut Entity) {
    entity.tick += 1;
    entity.anim += 1;

    if entity.anim == 3 {
        entity.anim = 0;

        if entity.tick & 1 == 0 {
            entity.sprite -= 0x3c;
        }

        entity.sprite += 0x1e;
    }
}

/// The walker's 10-frame cycle (`0x56b6..0x57c4`, 0x1e stride), every 4th.
fn walker_anim(entity: &mut Entity) {
    entity.anim += 1;

    if entity.anim == 4 {
        entity.anim = 0;
        entity.sprite = if entity.sprite == 0x57c4 {
            0x56b6
        } else {
            entity.sprite + 0x1e
        };
    }
}

/// The popper's cycle (`0x57e2` rest, 8-byte frames `0x5800..0x5810`).
fn popper_anim(entity: &mut Entity) {
    entity.tick += 1;
    entity.anim += 1;

    if entity.anim == 4 {
        entity.anim = 0;
        entity.sprite = match entity.sprite {
            0x57e2 => 0x5800,
            0x5810 => 0x57e2,
            other => other + 8,
        };
    }
}

/// One path-table step (the head of files `0x10104`/`0x101b5`/`0x1034c`):
/// adds the tick's `{dx, dy}` entry, scaled to 12.4. L3 paths never wrap,
/// and a long-lived follower CAN outrun its table (row 62 lives 426 ticks
/// on a 232-entry segment), harmlessly reading the next segment's bytes --
/// both engines read the same WAD data there.
fn path_step(entity: &mut Entity, wad: &[u8], segment: usize) {
    let at = segment_base(segment) + usize::from(entity.tick) * 4;
    entity.x += word(wad, at) << 4;
    entity.y += word(wad, at + 2) << 4;
}

/// Which orbit the three orbiters fly.
enum OrbitShape {
    /// Func 48: wave << 4 on both axes, x phase at twice the y rate.
    Single,
    /// Func 49: the same with both reads << 5.
    Double,
    /// Func 50: y << 5, x = save - (wave << 6) + 0x600, both phases +2.
    Wide,
}

/// The orbiters (funcs 48-50): approach, raise the gate, then orbit the
/// saved center on the shared wave table, with the attack animation gated
/// on vertical proximity to the player.
fn orbiter(entity: &mut Entity, ctx: &mut AiContext, approach_ticks: u16, shape: OrbitShape) {
    entity.tick += 1;

    if entity.tick <= approach_ticks {
        match shape {
            OrbitShape::Wide => entity.x -= 0x40,
            _ => entity.x -= 0x14,
        }

        copy_hitbox(entity, ctx.wad);

        return;
    }

    if entity.tick == approach_ticks + 1 {
        entity.phase_a = 0;
        entity.phase_b = match shape {
            OrbitShape::Wide => 0x3c,
            _ => 0,
        };
        entity.save_y = entity.y;
        entity.save_x = entity.x;
        *ctx.gate += 1;
    }

    let wave = |phase: u16| word(ctx.wad, ORBITER_WAVE + usize::from(phase));

    match shape {
        OrbitShape::Single => {
            entity.y = entity.save_y + (wave(entity.phase_a) << 4);
            entity.x = entity.save_x + (wave(entity.phase_b) << 4);
            entity.phase_a = (entity.phase_a + 2) % 0xf0;
            entity.phase_b = (entity.phase_b + 4) % 0xf0;
        }
        OrbitShape::Double => {
            entity.y = entity.save_y + (wave(entity.phase_a) << 5);
            entity.x = entity.save_x + (wave(entity.phase_b) << 5);
            entity.phase_a = (entity.phase_a + 2) % 0xf0;
            entity.phase_b = (entity.phase_b + 4) % 0xf0;
        }
        OrbitShape::Wide => {
            entity.y = entity.save_y + (wave(entity.phase_a) << 5);
            entity.x = entity.save_x - (wave(entity.phase_b) << 6) + 0x600;
            entity.phase_a = (entity.phase_a + 2) % 0xf0;
            entity.phase_b = (entity.phase_b + 2) % 0xf0;
        }
    }

    // The attack animation only starts near the player's row (the firing
    // plasma bypasses the check); once past the rest frame it runs the full
    // 16-frame cycle back to rest.
    if entity.sprite == 0x54d6
        && !ctx.firing_plasma
        && ((entity.y >> 4) - ctx.player_y).abs() > 0x28
    {
        copy_hitbox(entity, ctx.wad);

        return;
    }

    entity.anim += 1;

    if entity.anim == 3 {
        entity.anim = 0;
        entity.sprite = if entity.sprite == 0x5698 {
            0x54d6
        } else {
            entity.sprite + 0x1e
        };
    }

    copy_hitbox(entity, ctx.wad);
}

/// The slow drifter (func 51): starts animating once on screen (x <= 200 px)
/// and gains speed once past the rest frame.
fn slow_drifter(entity: &mut Entity, wad: &[u8]) {
    entity.x -= 0x10;

    if entity.x <= 0xc80 {
        entity.anim += 1;

        if entity.anim == 4 {
            entity.anim = 0;
            entity.sprite = if entity.sprite == 0x5a36 {
                0x5928
            } else {
                entity.sprite + 0x1e
            };
        }
    }

    if entity.sprite >= 0x5928 {
        entity.x -= 0xa;
    }

    copy_hitbox(entity, wad);
}

/// One leaper's scripted arc: per phase, the y step (12.4, negative = up)
/// and an optional frame to set.
struct LeapPhase {
    until: u16,
    dy: i32,
    sprite: Option<u16>,
}

/// Func 52's arc (steep), file `0x107b2`.
const LEAP_STEEP: &[LeapPhase] = &[
    LeapPhase {
        until: 0x7b,
        dy: -0x48,
        sprite: None,
    },
    LeapPhase {
        until: 0x80,
        dy: -0x38,
        sprite: None,
    },
    LeapPhase {
        until: 0x85,
        dy: -0x28,
        sprite: Some(0x5ae2),
    },
    LeapPhase {
        until: 0x8a,
        dy: -0x08,
        sprite: Some(0x5b00),
    },
    LeapPhase {
        until: 0x8f,
        dy: 0,
        sprite: Some(0x5b1e),
    },
    LeapPhase {
        until: 0x94,
        dy: 0x08,
        sprite: Some(0x5b3c),
    },
    LeapPhase {
        until: 0x99,
        dy: 0x28,
        sprite: Some(0x5b5a),
    },
    LeapPhase {
        until: 0x9e,
        dy: 0x38,
        sprite: None,
    },
    LeapPhase {
        until: 0xab,
        dy: 0x48,
        sprite: None,
    },
];

/// Func 53's arc (gentle), file `0x108b5`. (Its duplicated `tick <= 0x9e`
/// compare makes a second `+0x20` step unreachable; transcribed as taken.)
const LEAP_GENTLE: &[LeapPhase] = &[
    LeapPhase {
        until: 0x80,
        dy: -0x28,
        sprite: None,
    },
    LeapPhase {
        until: 0x85,
        dy: -0x20,
        sprite: None,
    },
    LeapPhase {
        until: 0x8a,
        dy: -0x10,
        sprite: Some(0x5ae2),
    },
    LeapPhase {
        until: 0x8f,
        dy: -0x08,
        sprite: Some(0x5b00),
    },
    LeapPhase {
        until: 0x94,
        dy: 0,
        sprite: Some(0x5b1e),
    },
    LeapPhase {
        until: 0x99,
        dy: 0x08,
        sprite: Some(0x5b3c),
    },
    LeapPhase {
        until: 0x9e,
        dy: 0x10,
        sprite: Some(0x5b5a),
    },
    LeapPhase {
        until: 0xb5,
        dy: 0x28,
        sprite: None,
    },
];

/// The splash effect templates (count byte + 13-byte rows). Template B's
/// count byte says 16 over 15 real rows; the 16th reads pad bytes and spawns
/// far off-screen, a faithful quirk.
const SPLASH_A: usize = 0x899f;
const SPLASH_B: usize = 0x8c78;

/// The leapers (funcs 52/53): parked off-screen until tick 0x71, then a
/// scripted dive out of the water at (175, 125) px with splash templates,
/// then parked at x 500 to cull. They hold their drift while the gate is
/// exactly 1.
fn leaper(entity: &mut Entity, ctx: &mut AiContext, arc: &[LeapPhase]) {
    entity.tick += 1;
    let tick = entity.tick;
    let last_splash = arc.last().map_or(0, |phase| phase.until) - 2;

    if tick == 1 {
        spawn_template(ctx, SPLASH_A, 0x11f, 0x96);
    }

    if tick == 0x39 {
        spawn_template(ctx, SPLASH_A, 0xe7, 0x96);
    }

    if tick == 0x71 {
        spawn_template(ctx, SPLASH_B, 0xaf, 0x7d);
        entity.x = 0xaf0;
        entity.y = 0x7d0;
    } else if tick > 0x71 {
        let phase = arc.iter().find(|phase| tick <= phase.until);

        match phase {
            Some(phase) => {
                entity.y += phase.dy;

                if let Some(sprite) = phase.sprite {
                    entity.sprite = sprite;
                }

                if tick == last_splash {
                    spawn_template(ctx, SPLASH_B, entity.x >> 4, 0x7d);
                }
            }
            None => entity.x = 0x1f40,
        }
    }

    if tick >= 0x71 && *ctx.gate != 1 {
        entity.x -= 0x10;
    }

    copy_hitbox(entity, ctx.wad);
}

/// Bursts an effect template (file `0x1073c`): a count byte then 13-byte
/// rows `{desc, dx, dy, frames, rate, step, phase, delay}` relative to the
/// burst's pixel position.
fn spawn_template(ctx: &mut AiContext, template: usize, x: i32, y: i32) {
    let Some(&count) = ctx.wad.get(template) else {
        return;
    };

    for row in 0..usize::from(count) {
        let at = template + 1 + row * 13;

        if ctx.wad.len() < at + 13 {
            break;
        }

        ctx.effects.push(Effect {
            sprite: u16::from_le_bytes([ctx.wad[at], ctx.wad[at + 1]]),
            x: x + word(ctx.wad, at + 2),
            y: y + word(ctx.wad, at + 4),
            frames: ctx.wad[at + 6],
            rate: ctx.wad[at + 7],
            step: u16::from_le_bytes([ctx.wad[at + 8], ctx.wad[at + 9]]),
            phase: ctx.wad[at + 10],
            delay: u16::from_le_bytes([ctx.wad[at + 11], ctx.wad[at + 12]]),
        });
    }
}

/// Boss data tables (file offsets).
const BOSS_BOB: usize = 0x109b9;
const BOSS_FRAME_DELTAS: usize = 0x109e1;
const BOSS_SINE: usize = 0x10a31;
const BOSS_LUNGE: usize = 0x10b21;

/// The level boss (func 54, file `0x10c7d`): fly in, raise the gate, swoop
/// to a hover anchor, then loop hover / lunge-left / wave / return. Health
/// is pinned at 15000 until tick 0x12c; below 2000 it bursts explosions; it
/// dies through the regular death handler at 0. No second form.
fn boss(entity: &mut Entity, ctx: &mut AiContext) {
    let state = &mut ctx.boss;

    // Every call: the global y-bob.
    state.bob_phase = (state.bob_phase + 1) % 0x28;

    if state.bob_phase & 1 == 0 {
        entity.y += word(ctx.wad, BOSS_BOB + state.bob_phase) << 4;
    }

    state.tick += 1;

    if state.tick <= 0x118 {
        entity.x -= 0x10;
        entity.health = 0x3a98;
    } else if state.tick <= 0x119 {
        *ctx.gate += 1;
        entity.health = 0x3a98;
    } else if state.tick <= 0x12c {
        entity.y -= 4;
        entity.health = 0x3a98;
    } else if state.tick <= 0x140 {
        entity.y -= 8;
        entity.x += 4;
    } else if state.tick <= 0x154 {
        entity.y -= 8;
        entity.x += 0xa;
    } else if state.tick <= 0x168 {
        entity.y -= 4;
        entity.x += 0xe;
    } else if state.tick <= 0x1a4 {
        entity.y -= 2;
        entity.x += 0x10;
        state.frame_index = 0;
        state.sine_index = 0;
        state.hover_count = 0;
        state.divider = 0;
        state.creep_x = entity.x;
        state.home_x = entity.x;
    } else if state.hover_count <= 0x1e0 {
        // HOVER: bob on the sine around a rightward-creeping anchor.
        state.pattern_count = 0;
        state.hover_count += 1;
        state.sine_index = (state.sine_index + 2) % 0xf0;
        state.frame_index = (state.frame_index + 1) % 0x50;

        if state.frame_index & 1 == 0 {
            let delta = word(ctx.wad, BOSS_FRAME_DELTAS + state.frame_index);
            entity.sprite = (i32::from(entity.sprite) + delta) as u16;
        }

        entity.x = state.creep_x + (word(ctx.wad, BOSS_SINE + state.sine_index) << 4);
        state.creep_x += 2;
    } else if state.pattern_count <= 0x28 {
        // LUNGE left ~200 px, stepping every 2nd call.
        state.pattern_count += 1;
        state.sine_index = 0;
        state.divider += 1;

        if state.divider >= 2 {
            let lunge = word(ctx.wad, BOSS_LUNGE + usize::from(state.pattern_count) * 2);
            entity.x = state.creep_x + lunge - 0x63f;
            state.divider = 0;
            entity.sprite += 0x1e;

            if entity.sprite == 0x5c3e {
                entity.sprite += 0x1e;
            }
        }

        state.lunge_end_x = entity.x;
        state.frame_index = 0;
    } else if state.pattern_count <= 0xc8 {
        // WAVE at the lunge end.
        state.pattern_count += 1;
        state.sine_index = (state.sine_index + 2) % 0xf0;
        entity.x = state.lunge_end_x + (word(ctx.wad, BOSS_SINE + state.sine_index) << 4);
    } else if entity.x <= state.home_x {
        // RETURN right.
        entity.x += 0x10;
    } else if state.pattern_count <= 0xf0 {
        // Animate back to the rest pose, every 2nd call.
        state.pattern_count += 1;

        if state.pattern_count & 1 == 0 {
            if entity.sprite == 0x60d0 {
                entity.sprite = 0x5c20;
            } else {
                entity.sprite += 0x1e;

                if entity.sprite == 0x5c3e {
                    entity.sprite += 0x1e;
                }
            }
        }
    } else {
        // RESET: loop the pattern from the hover.
        state.sine_index = 0;
        state.hover_count = 0;
        state.divider = 0;
        state.frame_index = 0;
        state.tick = 0x1a5;
        state.creep_x = state.home_x;
        entity.x = state.home_x;
        entity.sprite = 0x5c20;
    }

    // Every call after the phase machine: the volley timer.
    state.fire_timer -= 1;

    if state.fire_timer == 0 {
        state.fire_timer = 0x28;

        if entity.sprite <= 0x5c5c {
            fire_volley(entity, ctx, VolleyFacing::Left);
        }

        if entity.sprite == 0x5e96 {
            fire_volley(entity, ctx, VolleyFacing::Right);
        }
    }

    boss_explosions(entity, ctx);
    copy_hitbox(entity, ctx.wad);
}

enum VolleyFacing {
    Left,
    Right,
}

/// One 3-shot volley (files `0x10eeb`/`0x10f86`): an aimed shot from the
/// muzzle plus two straight shots, with the volley voice sample.
fn fire_volley(entity: &mut Entity, ctx: &mut AiContext, facing: VolleyFacing) {
    let (aim_dx, aim_dy) = match facing {
        VolleyFacing::Left => (0x440, 0x450),
        VolleyFacing::Right => (0x910, 0x450),
    };

    let muzzle_x = (entity.x + aim_dx) >> 4;
    let muzzle_y = (entity.y + aim_dy) >> 4;

    if let Some((vx, vy)) = aim_at_player(ctx, muzzle_x, muzzle_y) {
        ctx.shots.push(Shot {
            sprite: 0x4e8e,
            x: entity.x + aim_dx,
            y: entity.y + aim_dy,
            vx,
            vy,
        });
    }

    match facing {
        VolleyFacing::Left => {
            ctx.shots.push(Shot {
                sprite: 0x4ea2,
                x: entity.x + 0x330,
                y: entity.y + 0x240,
                vx: -0x40,
                vy: 0,
            });
            ctx.shots.push(Shot {
                sprite: 0x4e8e,
                x: entity.x + 0x700,
                y: entity.y + 0x510,
                vx: -0x40,
                vy: 0x30,
            });
        }
        VolleyFacing::Right => {
            ctx.shots.push(Shot {
                sprite: 0x4ea2,
                x: entity.x + 0xa10,
                y: entity.y + 0x240,
                vx: 0x40,
                vy: 0,
            });
            ctx.shots.push(Shot {
                sprite: 0x4e8e,
                x: entity.x + 0x650,
                y: entity.y + 0x510,
                vx: 0x40,
                vy: 0x30,
            });
        }
    }

    ctx.sounds.push(SLOT_VOLLEY);
}

/// The aimed-shot helper (file `0x117a7`): a velocity toward the player's
/// center at 4 px per step (12.4 per sub-step).
///
/// `None` at point-blank range: the original's idiv is unguarded and a
/// zero scale crashes it with a divide fault (reachable while shielded
/// inside the boss); the port's deviation is to skip the shot, the same
/// shape on every level.
fn aim_at_player(ctx: &mut AiContext, shooter_x: i32, shooter_y: i32) -> Option<(i32, i32)> {
    let diff_x = ctx.player_x + 0x1e + 0xa - shooter_x;
    let diff_y = ctx.player_y + 0xa + 0xa - shooter_y;
    let dist =
        (i64::from(diff_x) * i64::from(diff_x) + i64::from(diff_y) * i64::from(diff_y)).isqrt();
    let scale = (dist as i32) / 4;

    if scale == 0 {
        return None;
    }

    Some(((diff_x << 4) / scale, (diff_y << 4) / scale))
}

/// The boss's continuous explosion bursts below 2000 health (file `0x10bc1`).
fn boss_explosions(entity: &mut Entity, ctx: &mut AiContext) {
    if entity.health > 0x7d0 {
        return;
    }

    let state = &mut ctx.boss;
    state.explosion_timer -= 1;

    if state.explosion_timer > 0 {
        return;
    }

    *ctx.boss_explosion = Some(BossExplosionSound::Explosion);
    state.explosion_timer = i32::from(ctx.rng.next(0x14));
    state.explosion_dx = i32::from(ctx.rng.next(0x82)) + 0x1e;
    state.explosion_dy = i32::from(ctx.rng.next(0x3c)) + 0x14;

    ctx.effects.push(Effect {
        sprite: 0x53bc,
        x: (entity.x >> 4) + state.explosion_dx,
        y: (entity.y >> 4) + state.explosion_dy,
        frames: 9,
        rate: 3,
        step: 8,
        phase: 0,
        delay: 0,
    });
}

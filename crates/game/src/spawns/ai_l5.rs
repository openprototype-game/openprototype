//! LEVEL_5's 44 mode-0 enemy AI functions, transcribed from the disassembly
//! (`re/l5-ai-functions.md`; pointer table at file `0xeb55`).
//!
//! Same engine as L1 with a different AI set. All enemy fire is
//! deterministic (no random-fire helper); the boss is a stationary fixture
//! whose facing reacts to the player's position and steering keys. The
//! walker/fixture family (funcs 40/41) and the boss step 0x1e-byte
//! descriptors and re-copy their hitboxes per sub-step; everything else uses
//! L1-style 8-byte cycle frames.

use super::{AiSounds, BossExplosionSound, Effect, Entity, Shot, descriptor_hitboxes};
use crate::level::prng::EngineRng;

/// LEVEL_5's cs-pointer to file-offset base.
const CS_BASE: usize = 0x3f90;

/// The volley sample (the level's extra 17th slot, trigger id `0x11`).
const SLOT_VOLLEY: usize = 16;

/// The boss phase-change sample (kanone, the per-level slot 8).
const SLOT_PHASE_CHANGE: usize = 8;

/// Per-step context the AI functions read and write besides the entity.
pub(super) struct AiContext<'a> {
    pub wad: &'a [u8],
    pub rng: &'a mut EngineRng,
    /// Player position in pixels (camera-inclusive buffer coordinates).
    pub player_x: i32,
    pub player_y: i32,
    pub shots: &'a mut Vec<Shot>,
    pub effects: &'a mut Vec<Effect>,
    pub boss: &'a mut BossState,
    /// The boss/scroll gate (`cs:0x2689`).
    pub gate: &'a mut u8,
    /// A boss explosion burst fired this step.
    pub boss_explosion: &'a mut Option<BossExplosionSound>,
    /// Sample slots the AI triggered this step (event channel).
    pub sounds: &'a mut AiSounds,
    /// Whether the firing weapon is the plasma (`cs:0xcb5 == 3`); the boss
    /// fires four times faster against it.
    pub firing_plasma: bool,
    /// Whether a left/right arrow is held (`cs:0x820c/0x820d`); the boss
    /// holds its facing while the player is steering mid-screen.
    pub steering: bool,
}

/// The boss's engine globals (`cs:0xce4..0xced`), all file-image zero.
#[derive(Default)]
pub(super) struct BossState {
    /// The boss tick (`cs:0xce4`) and the fire-tick counter (`cs:0xce9`).
    tick: u16,
    fire_ticks: u16,
    /// Facing phase (`cs:0xce6`): 0 = left, 1 = right, 2 = turning to
    /// right, 3 = turning to left.
    phase: u8,
    /// The turn's bounce sub-state (`cs:0xce8`, 0..2).
    bounce: u8,
    /// The half-rate toggle (`cs:0xce7`).
    half_rate: u8,
    /// Explosion-burst timer (`cs:0xa734`, file-image zero) and offsets.
    explosion_timer: i32,
    explosion_dx: i32,
    explosion_dy: i32,
}

/// Reads an i16 word from the WAD image.
fn word(wad: &[u8], at: usize) -> i32 {
    if wad.len() < at + 2 {
        return 0;
    }

    i32::from(i16::from_le_bytes([wad[at], wad[at + 1]]))
}

/// Re-copies the current frame's hitboxes (the 0x1e-stride families).
fn copy_hitbox(entity: &mut Entity, wad: &[u8]) {
    entity.hitboxes = descriptor_hitboxes(wad, CS_BASE, entity.sprite);
}

/// The 15-entry y-bob table (file `0xdd81`), byte-indexed, wrap 0x1e.
const BOB_TABLE: usize = 0xdd81;

/// The swooper y-delta table (file `0xe1be`): 181 words, no wrap (overruns
/// read code bytes and fling the entity past the cull, faithfully).
const SWOOP_TABLE: usize = 0xe1be;

/// The left-moving path segments shared by funcs 8-16 and 17-25.
const LEFT_SEGMENTS: [usize; 9] = [0x38, 0x51, 0x77, 0xa9, 0xdb, 0x11a, 0x159, 0x18b, 0x1c5];

/// The right-moving path segments (funcs 26-36; spawns enter at x -20).
const RIGHT_SEGMENTS: [usize; 11] = [
    0x1f7, 0x229, 0x24f, 0x275, 0x29b, 0x2c1, 0x2e7, 0x308, 0x32e, 0x34f, 0x370,
];

/// A path segment's file offset (`segment * 16 + 0x200`).
fn segment_base(segment: usize) -> usize {
    segment * 16 + 0x200
}

/// Runs AI function `arg` for one sub-step.
pub(super) fn step(entity: &mut Entity, ctx: &mut AiContext) {
    match entity.arg {
        0 => pickup(entity, 0x3764, 0x37c2),
        1 => pickup(entity, 0x3688, 0x36e6),
        2 => pickup(entity, 0x36ee, 0x375c),
        3 => pickup(entity, 0x382a, 0x3870),
        4 => dasher(entity),
        5 => turret(entity, ctx),
        7 => {
            entity.x -= 0x30;
            cycler_a(entity);
        }
        8..=16 => {
            path_step(entity, ctx.wad, LEFT_SEGMENTS[usize::from(entity.arg) - 8]);
            shooter_anim(entity, ctx);
        }
        17..=25 => {
            path_step(entity, ctx.wad, LEFT_SEGMENTS[usize::from(entity.arg) - 17]);
            cycler_a(entity);
        }
        26..=36 => {
            path_step(
                entity,
                ctx.wad,
                RIGHT_SEGMENTS[usize::from(entity.arg) - 26],
            );
            cycler_b(entity);
        }
        37 => {
            entity.x += 0x30;
            cycler_b(entity);
        }
        38 => swooper(entity, ctx.wad, false),
        39 => swooper(entity, ctx.wad, true),
        40 => fixture(entity, ctx),
        41 => walker(entity, ctx),
        42 => sweeper(entity, ctx),
        43 => boss(entity, ctx),
        _ => {}
    }
}

/// Pickup drifters (funcs 0-3): drift left at 1.25 px, period-4 cycle
/// `rest -> rest+0x1e -> +8 steps -> last -> rest`.
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

/// Func 4: the dasher (the shooter sprite flying straight left fast, no
/// fire).
fn dasher(entity: &mut Entity) {
    entity.x -= 0x50;
    entity.tick += 1;
    entity.anim += 1;

    if entity.anim == 4 {
        entity.anim = 0;
        entity.sprite = match entity.sprite {
            0x3a2c => 0x3a4a,
            0x3a72 => 0x3a2c,
            other => other + 8,
        };
    }
}

/// Func 5: the rotating turret. Approaches with a y-bob, anchors, then
/// cycles a five-direction firing sweep; a raised gate makes it engage
/// earlier and hold position.
fn turret(entity: &mut Entity, ctx: &mut AiContext) {
    entity.phase_a += 2;

    if entity.phase_a >= 0x1e {
        entity.phase_a = 0;
    }

    entity.y += word(ctx.wad, BOB_TABLE + usize::from(entity.phase_a)) << 4;
    entity.tick += 1;

    let engage_at = if *ctx.gate != 0 { 0x78 } else { 0x10e };

    if entity.tick <= engage_at {
        entity.x -= 0xc;
        entity.anim += 1;

        if entity.anim == 4 {
            entity.anim = 0;
            entity.sprite = match entity.sprite {
                0x3ac2 => 0x3ae0,
                0x3b10 => 0x3ac2,
                other => other + 8,
            };
        }

        return;
    }

    if *ctx.gate == 0 {
        entity.x += 4;
    }

    entity.anim += 1;

    if entity.anim <= 4 {
        return;
    }

    entity.anim = 0;

    let shot = match entity.sprite {
        0x3b18 => Some((entity.x + 0x280, entity.y + 0x250, 0, 0x40)),
        0x3b28 => Some((entity.x + 0x120, entity.y + 0x220, -0x38, 0x30)),
        0x3b38 => Some((entity.x + 0xa0, entity.y + 0x150, -0x40, 0)),
        0x3b48 => Some((entity.x + 0x110, entity.y + 0x80, -0x38, -0x30)),
        0x3b58 => Some((entity.x + 0x2a0, entity.y + 0x40, 0, -0x40)),
        _ => None,
    };

    if let Some((x, y, vx, vy)) = shot {
        ctx.shots.push(Shot {
            sprite: 0x3a18,
            x,
            y,
            vx,
            vy,
        });
    }

    entity.sprite = if entity.sprite == 0x3b68 {
        0x3b18
    } else {
        entity.sprite + 8
    };
}

/// Cycler A (file `0xdfdd`): `0x3c4e -> 0x3c6c .. 0x3c7c -> 0x3c4e`,
/// period 4.
fn cycler_a(entity: &mut Entity) {
    entity.anim += 1;

    if entity.anim == 4 {
        entity.anim = 0;
        entity.sprite = match entity.sprite {
            0x3c4e => 0x3c6c,
            0x3c7c => 0x3c4e,
            other => other + 8,
        };
    }
}

/// Cycler B (file `0xe002`): `0x3c84 -> 0x3ca2 .. 0x3cb2 -> 0x3c84`,
/// period 4.
fn cycler_b(entity: &mut Entity) {
    entity.anim += 1;

    if entity.anim == 4 {
        entity.anim = 0;
        entity.sprite = match entity.sprite {
            0x3c84 => 0x3ca2,
            0x3cb2 => 0x3c84,
            other => other + 8,
        };
    }
}

/// The shooter's two-phase ping-pong anim with one shot per long cycle
/// (file `0xdeb7`).
fn shooter_anim(entity: &mut Entity, ctx: &mut AiContext) {
    entity.anim += 1;

    if entity.anim != 4 {
        return;
    }

    entity.anim = 0;

    if entity.phase_a == 0 {
        entity.sprite = match entity.sprite {
            0x3a2c => 0x3a4a,
            0x3a72 => {
                entity.phase_a += 1;
                0x3a2c
            }
            other => other + 8,
        };
    } else {
        entity.sprite = match entity.sprite {
            0x3a2c => 0x3a4a,
            0x3aba => {
                entity.phase_a = 0;
                0x3a2c
            }
            other => {
                let next = other + 8;

                if next == 0x3a7a {
                    ctx.shots.push(Shot {
                        sprite: 0x3a18,
                        x: entity.x + 0x50,
                        y: entity.y + 0x70,
                        vx: -0x60,
                        vy: 0,
                    });
                }

                next
            }
        };
    }
}

/// One path-table step: increments the tick, then adds its `{dx, dy}` entry
/// scaled to 12.4. No wrap, like all L5 paths.
fn path_step(entity: &mut Entity, wad: &[u8], segment: usize) {
    entity.tick += 1;
    let at = segment_base(segment) + usize::from(entity.tick) * 4;
    entity.x += word(wad, at) << 4;
    entity.y += word(wad, at + 2) << 4;
}

/// Funcs 38/39: the swoopers. Straight left at 2 px with a table-driven y
/// curve (negated for func 39) and a tilt animation every 6 sub-steps.
fn swooper(entity: &mut Entity, wad: &[u8], mirrored: bool) {
    entity.x -= 0x20;
    entity.tick += 1;

    let mut dy = word(wad, SWOOP_TABLE + usize::from(entity.tick) * 2) << 4;

    if mirrored {
        dy = -dy;
    }

    entity.y += dy;
    tilt_anim(entity, dy);
}

/// The swooper's tilt (file `0xe328`): every 6 sub-steps the sprite tilts
/// toward the motion direction, easing back to the rest pose when level.
fn tilt_anim(entity: &mut Entity, dy: i32) {
    entity.anim += 1;

    if entity.anim < 6 {
        return;
    }

    entity.anim = 0;

    if dy > 0 {
        if entity.sprite == 0x3cf0 {
            entity.sprite = 0x3d0e;
        } else if entity.sprite >= 0x3d26 {
            neutral(entity);
        } else if entity.sprite < 0x3d1e {
            entity.sprite += 8;
        }
    } else if dy < 0 {
        if entity.sprite <= 0x3cf0 {
            entity.sprite = 0x3d26;
        } else if entity.sprite < 0x3d3e {
            entity.sprite += 8;
        }
    } else {
        neutral(entity);
    }

    fn neutral(entity: &mut Entity) {
        if entity.sprite == 0x3cf0 {
            return;
        }

        if entity.sprite == 0x3d26 || entity.sprite == 0x3d0e {
            entity.sprite = 0x3cf0;
        } else {
            entity.sprite -= 8;
        }
    }
}

/// Func 40: the fixture turret. Drifts left alternating two 0x1e frames and
/// lobs a slow falling bomb every 40 sub-steps; its debris stays the
/// one-row fixture pop.
fn fixture(entity: &mut Entity, ctx: &mut AiContext) {
    entity.x -= 0x1c;
    entity.tick += 1;
    entity.anim += 1;

    if entity.anim >= 2 {
        entity.anim = 0;
        entity.sprite += 0x1e;

        if entity.sprite >= 0x3d82 {
            entity.sprite = 0x3d46;
        }
    }

    if entity.tick >= 0x28 {
        entity.tick = 0;
        ctx.shots.push(Shot {
            sprite: 0x3a22,
            x: entity.x + 0x320,
            y: entity.y + 0x1e0,
            vx: -0x40,
            vy: 4,
        });
    }

    entity.debris = 0x2f83;
    copy_hitbox(entity, ctx.wad);
}

/// Func 41: the walker. Decelerating entry with a y-bob, then a 0x1e-stride
/// walk cycle with frame-keyed fire; when the gate is down it morphs back to
/// the fixture pose and retreats along path 0x18b.
///
/// Field mapping: the path index is `phase_a` (entity +0x22), the form flag
/// is `phase_b` (the original's byte at +0x24), and the bob phase rides in
/// `save_y` (the original's overlapping word at +0x25, which nothing else in
/// this function touches).
fn walker(entity: &mut Entity, ctx: &mut AiContext) {
    entity.tick += 1;

    if entity.tick <= 1 {
        entity.save_y = 0;
        entity.phase_b = 0;
        entity.phase_a = 0;
    } else {
        entity.save_y += 2;

        if entity.save_y >= 0x1e {
            entity.save_y = 0;
        }

        entity.y += word(ctx.wad, BOB_TABLE + entity.save_y as usize) << 4;

        if entity.tick <= 0x8c {
            entity.x -= 0x12;
        } else if entity.tick <= 0x96 {
            entity.x -= 0x0c;
        } else if entity.tick <= 0xa0 {
            entity.x -= 0x08;
        } else if entity.tick <= 0xaa {
            entity.x -= 0x04;
        } else if entity.phase_b == 0x11 {
            retreat(entity, ctx.wad);
        } else {
            walk_cycle(entity, ctx);
        }
    }

    entity.debris = if entity.sprite >= 0x3e72 {
        0x2f91
    } else {
        0x2f83
    };
    copy_hitbox(entity, ctx.wad);
}

/// The walker's walk loop: frames every 4 sub-steps, firing on specific
/// frames; the cycle ends back at 0x4052, retreating once the gate is down.
fn walk_cycle(entity: &mut Entity, ctx: &mut AiContext) {
    entity.anim += 1;

    if entity.anim < 4 {
        return;
    }

    entity.anim = 0;
    entity.sprite += 0x1e;

    let shot = match entity.sprite {
        0x40ac => Some((0x3a18, entity.x + 0x3a0, entity.y + 0xc0, -0x50)),
        0x4106 => Some((0x3a18, entity.x + 0x300, entity.y + 0x170, -0x50)),
        0x4160 => Some((0x3414, entity.x + 0x280, entity.y + 0x2e0, -0x40)),
        0x41d8 => Some((0x341e, entity.x + 0x290, entity.y + 0x230, -0x40)),
        _ => None,
    };

    if let Some((sprite, x, y, vx)) = shot {
        ctx.shots.push(Shot {
            sprite,
            x,
            y,
            vx,
            vy: 0,
        });
    }

    if entity.sprite >= 0x4250 {
        entity.sprite = 0x4052;

        if *ctx.gate == 0 {
            entity.phase_a = 0;
            entity.phase_b = 0x11;
        }
    }
}

/// The walker's retreat: morph back down to the fixture frame, then follow
/// path segment 0x18b off screen (one entry per call).
fn retreat(entity: &mut Entity, wad: &[u8]) {
    if entity.sprite != 0x3d46 {
        entity.anim += 1;

        if entity.anim >= 4 {
            entity.anim = 0;
            entity.sprite -= 0x1e;
        }
    } else {
        entity.phase_a += 1;
        let at = segment_base(0x18b) + usize::from(entity.phase_a) * 4;
        entity.x += word(wad, at) << 4;
        entity.y += word(wad, at + 2) << 4;
    }
}

/// Func 42: the sweeper mini-boss. Raises the gate on its first tick,
/// approaches to x 120, then sweeps between x -20 and 150 firing an up-left
/// fan once per 16-frame cycle. The fields: `phase_a` = engaged flag,
/// `phase_b` = direction (1 = moving left).
fn sweeper(entity: &mut Entity, ctx: &mut AiContext) {
    entity.tick += 1;

    if entity.tick <= 1 {
        entity.phase_a = 0;
        entity.phase_b = 0;
        *ctx.gate += 1;

        return;
    }

    entity.anim += 1;

    if entity.phase_a == 0 {
        if entity.anim >= 5 {
            entity.anim = 0;

            if entity.sprite == 0x3b70 {
                entity.sprite = 0x3b8e;
            } else if entity.sprite == 0x3bbe {
                entity.sprite = 0x3b70;

                if entity.x <= 0x780 {
                    entity.phase_a = 1;
                    entity.phase_b = 1;
                    entity.sprite = 0x3bbe;
                }
            } else {
                entity.sprite += 8;
            }
        }

        entity.x -= 8;

        return;
    }

    if entity.anim >= 5 {
        entity.anim = 0;
        entity.sprite = if entity.sprite == 0x3c3e {
            0x3bc6
        } else {
            entity.sprite + 8
        };
    }

    if entity.sprite == 0x3bc6 && entity.anim == 0 {
        let (x, y) = (entity.x + 0x380, entity.y + 0x100);

        for (sprite, vx, vy) in [
            (0x3414, -0x47, -0x23),
            (0x340a, -0x32, -0x2a),
            (0x340a, -0x4d, -0x15),
            (0x340a, -0x19, -0x2d),
            (0x340a, -0x4c, 0),
        ] {
            ctx.shots.push(Shot {
                sprite,
                x,
                y,
                vx,
                vy,
            });
        }

        ctx.sounds.push(SLOT_VOLLEY);
    }

    if entity.phase_b != 1 {
        entity.x += 8;

        if entity.sprite == 0x3bc6 && entity.x >= 0x960 {
            entity.phase_b = 1;
        }
    } else {
        entity.x -= 8;

        if entity.sprite == 0x3bc6 && entity.x <= -0x140 {
            entity.phase_b = 0;
        }
    }
}

/// Func 43: the level boss, a stationary fixture. After the fly-in it holds
/// the gate up every call and runs a facing state machine keyed on the
/// player's x position and steering keys, with aimed volleys (four times
/// faster while the player fires the plasma).
fn boss(entity: &mut Entity, ctx: &mut AiContext) {
    ctx.boss.tick += 1;
    ctx.boss.fire_ticks += 1;

    if ctx.boss.tick <= 0x91 {
        entity.x -= 0x18;
        entity.health = 0x3a98;
        boss_tail(entity, ctx);

        return;
    }

    *ctx.gate = 1;

    // Outside the turn's bounce the machine runs at half rate.
    if ctx.boss.bounce == 0 {
        ctx.boss.half_rate += 1;

        if ctx.boss.half_rate < 2 {
            boss_tail(entity, ctx);

            return;
        }

        ctx.boss.half_rate = 0;
    }

    let mid_screen = (0x5a..=0x96).contains(&ctx.player_x);

    match ctx.boss.phase {
        0 => {
            if fire_due(ctx) {
                volley_a(entity, ctx);
            }

            if ctx.player_x >= 0x96 || (mid_screen && !ctx.steering) {
                ctx.boss.phase = 2;
                ctx.sounds.push(SLOT_PHASE_CHANGE);
            }
        }
        1 => {
            if fire_due(ctx) {
                volley_b(entity, ctx);
            }

            if ctx.player_x <= 0x5a || (mid_screen && !ctx.steering) {
                ctx.boss.phase = 3;
                ctx.sounds.push(SLOT_PHASE_CHANGE);
            }
        }
        2 => {
            let mut bounce = ctx.boss.bounce;

            if bounce == 0 {
                if entity.sprite == 0x4322 && mid_screen {
                    // Start the bounce; the original falls straight into
                    // the rise in the same call.
                    bounce = 1;
                } else if entity.sprite >= 0x43d6 {
                    ctx.boss.phase = 1;
                } else {
                    entity.sprite += 0x1e;
                }
            }

            if bounce == 1 {
                entity.y -= 0x20;

                if entity.y <= 0xa0 {
                    bounce = 2;
                }
            } else if bounce == 2 {
                entity.y += 0x20;

                if entity.y == 0x460 {
                    bounce = 0;
                    entity.sprite += 0x1e;
                }
            }

            ctx.boss.bounce = bounce;
        }
        _ => {
            if entity.sprite >= 0x4520 {
                entity.sprite = 0x426e;
                ctx.boss.phase = 0;
            } else {
                entity.sprite += 0x1e;
            }
        }
    }

    boss_tail(entity, ctx);
}

/// The boss's per-call tail: hitbox copy and the explosion bursts.
fn boss_tail(entity: &mut Entity, ctx: &mut AiContext) {
    boss_explosions(entity, ctx);
    copy_hitbox(entity, ctx.wad);
}

/// Whether the volley timer expired (`0x50` fire-ticks, `0x14` against the
/// firing plasma); firing resets it.
fn fire_due(ctx: &mut AiContext) -> bool {
    let due = ctx.boss.fire_ticks >= 0x50 || (ctx.firing_plasma && ctx.boss.fire_ticks >= 0x14);

    if due {
        ctx.boss.fire_ticks = 0;
    }

    due
}

/// The aimed-shot helper (file `0xf286`): velocity toward the player's
/// center at 3 px per step.
fn aim_at_player(ctx: &AiContext, shooter_x: i32, shooter_y: i32) -> (i32, i32) {
    let diff_x = ctx.player_x + 0x1e + 0xa - shooter_x;
    let diff_y = ctx.player_y + 0xa + 0xa - shooter_y;
    let dist =
        (i64::from(diff_x) * i64::from(diff_x) + i64::from(diff_y) * i64::from(diff_y)).isqrt();
    let scale = ((dist as i32) / 3).max(1);

    ((diff_x << 4) / scale, (diff_y << 4) / scale)
}

/// Volley A (file `0xe806`, facing left): one aimed shot, an up-left fan,
/// and a rear up-right shot.
fn volley_a(entity: &mut Entity, ctx: &mut AiContext) {
    let (vx, vy) = aim_at_player(ctx, (entity.x + 0x2b0) >> 4, (entity.y + 0x420) >> 4);
    ctx.shots.push(Shot {
        sprite: 0x340a,
        x: entity.x + 0x2b0,
        y: entity.y + 0x420,
        vx,
        vy,
    });

    for (vx, vy) in [
        (-0x48, -0x20),
        (-0x48, -0x30),
        (-0x32, -0x38),
        (-0x40, -0x8),
        (-0x30, 0xc),
    ] {
        ctx.shots.push(Shot {
            sprite: 0x341e,
            x: entity.x - 0x20,
            y: entity.y + 0xd0,
            vx,
            vy,
        });
    }

    ctx.shots.push(Shot {
        sprite: 0x340a,
        x: entity.x + 0x880,
        y: entity.y + 0x170,
        vx: 0x40,
        vy: -0x40,
    });
    ctx.sounds.push(SLOT_VOLLEY);
}

/// Volley B (file `0xe974`, facing right): the aimed shot plus a fan whose
/// even shots fire from 167 px left of the boss — verbatim from the
/// original (a likely sign slip, kept faithful).
fn volley_b(entity: &mut Entity, ctx: &mut AiContext) {
    let (vx, vy) = aim_at_player(ctx, (entity.x + 0x7c0) >> 4, (entity.y + 0x430) >> 4);
    ctx.shots.push(Shot {
        sprite: 0x340a,
        x: entity.x + 0x7c0,
        y: entity.y + 0x430,
        vx,
        vy,
    });

    for (index, (vx, vy)) in [
        (0x48, -0x20),
        (0x48, -0x30),
        (0x32, -0x38),
        (0x40, -0x8),
        (0x30, 0xc),
    ]
    .into_iter()
    .enumerate()
    {
        let muzzle_x = if index % 2 == 0 { 0xa70 } else { -0xa70 };
        ctx.shots.push(Shot {
            sprite: 0x341e,
            x: entity.x + muzzle_x,
            y: entity.y + 0x10,
            vx,
            vy,
        });
    }

    ctx.shots.push(Shot {
        sprite: 0x340a,
        x: entity.x + 0x200,
        y: entity.y + 0x170,
        vx: -0x40,
        vy: -0x40,
    });
    ctx.sounds.push(SLOT_VOLLEY);
}

/// The boss's continuous explosion bursts below 2000 health (file `0xe6ca`).
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
    state.explosion_dx = i32::from(ctx.rng.next(0x28)) + 0xf;
    state.explosion_dy = i32::from(ctx.rng.next(0x1e)) + 0xf;

    ctx.effects.push(Effect {
        sprite: 0x3938,
        x: (entity.x >> 4) + state.explosion_dx,
        y: (entity.y >> 4) + state.explosion_dy,
        frames: 9,
        rate: 3,
        step: 8,
        phase: 0,
        delay: 0,
    });
}

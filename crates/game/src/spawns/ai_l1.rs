//! LEVEL_1's 24 mode-0 enemy AI functions, transcribed from the disassembly
//! (`re/l1-ai-functions.md`; pointer table at file `0xd6c3`).
//!
//! Each function runs once per movement sub-step with the entity's registers
//! loaded; positions are 12.4 fixed point. The data tables (paths, waves,
//! wobbles) are read straight from the WAD image at their file offsets, like
//! the sprite descriptors.
//!
//! Side effects the port does not carry yet are marked `TODO` at their sites:
//! the effects/spawn queue (func 4's child drop, the boss explosions), the
//! per-function SFX triggers, the boss/orbiter scroll gate (`cs:0x269c`), the
//! orbiter frame patch's hitbox/claw writes, and the firing-weapon gate bypass
//! (`cs:0xcb5 == 3`).

use super::{AiSounds, BossExplosionSound, Effect, Entity, Shot, descriptor_hitboxes};
use crate::level::prng::EngineRng;

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
    /// The boss/orbiter gate counter (`cs:0x269c`).
    pub gate: &'a mut u8,
    /// A boss explosion fired this step (the form picks its sounds).
    pub boss_explosion: &'a mut Option<BossExplosionSound>,
    /// Sample slots the AI triggered this step (event channel).
    pub sounds: &'a mut AiSounds,
    /// Live patches to the shared per-kind descriptors' debris slots.
    pub debris_overrides: &'a mut std::collections::HashMap<u16, u16>,
}

/// The boss's engine globals (`cs:0x269d..0x26a7`, `cs:0xce8/0xce9`); one boss
/// runs at a time, so the original keeps these outside the entity.
pub(super) struct BossState {
    anchor_x: i32,
    anchor_y: i32,
    saved_a: u16,
    saved_b: u16,
    fire_timer: i32,
    explosion_timer: i32,
    form2: bool,
    dying: bool,
}

impl Default for BossState {
    fn default() -> Self {
        Self {
            anchor_x: 0,
            anchor_y: 0,
            saved_a: 0,
            saved_b: 0,
            // The WAD image bakes both timers' starting values (cs:0x26a5 =
            // 0x3c, cs:0x26a7 = 0x28); zero defaults fired the first volley
            // 0x3b sub-steps early and the first death burst 0x27 early.
            fire_timer: 0x3c,
            explosion_timer: 0x28,
            form2: false,
            dying: false,
        }
    }
}

/// The carrier pod's deploy sample (gegrocke, the per-level slot 8).
const SLOT_ENEMY_VOICE: usize = 8;

/// Reads an i16 word from the WAD image.
fn word(wad: &[u8], at: usize) -> i32 {
    i32::from(i16::from_le_bytes([wad[at], wad[at + 1]]))
}

/// Runs AI function `arg` for one sub-step.
pub(super) fn step(entity: &mut Entity, ctx: &mut AiContext) {
    match entity.arg {
        0 => asteroid(entity, -0x20, 4),
        1 => asteroid(entity, -0x18, 6),
        2 => asteroid(entity, -0x1c, 5),
        3 => turret(entity, ctx),
        4 => carrier_pod(entity, ctx),
        5 => flapper(entity, -0x14, 4, 0x36ea, 0x3708, 0x3748),
        6 => {
            entity.x -= 0xc;
            shooter_anim(entity, ctx);
        }
        7 => orbiter(entity, ctx, OrbiterShape::Circle),
        8 => orbiter(entity, ctx, OrbiterShape::Lissajous),
        9 => path_popper(entity, ctx, 0x6b0, 0xfa),
        10 => path_popper(entity, ctx, 0xaa0, 0xc8),
        11 => path_popper(entity, ctx, 0xdc0, 0x12c),
        12 => path_popper(entity, ctx, 0x10e0, 0x12c),
        13 => {
            entity.x -= 0x30;
            popper_anim(entity);
        }
        14 => path_shooter(entity, ctx, 0x1590, 0xfa),
        15 => path_shooter(entity, ctx, 0x1980, 0xfa),
        16 => path_shooter(entity, ctx, 0x1d70, 0x12c),
        17 => path_shooter(entity, ctx, 0x2220, 0xc8),
        18 => bomber(entity, ctx),
        19 => flapper(entity, -0x14, 4, 0x3750, 0x376e, 0x37ae),
        20 => flapper(entity, -0x14, 4, 0x37b6, 0x37d4, 0x3824),
        21 => flapper(entity, -0x14, 4, 0x382c, 0x3846, 0x386e),
        22 => boss(entity, ctx),
        23 => {
            // Form 2 is the same body with the form flag set (`cs:0xce8`).
            ctx.boss.form2 = true;
            boss(entity, ctx);
        }
        _ => {}
    }
}

/// Functions 0-2: drift left, cycle the 0x3308 asteroid frames (the `0xc5e5`
/// cycler) every `threshold` sub-steps.
fn asteroid(entity: &mut Entity, speed: i32, threshold: u8) {
    entity.x += speed;
    entity.anim += 1;

    if entity.anim == threshold {
        entity.anim = 0;

        if entity.sprite == 0x3308 {
            entity.sprite = 0x3326;
        } else if entity.sprite == 0x3386 {
            entity.sprite = 0x3308;
        } else {
            entity.sprite += 8;
        }
    }
}

/// Functions 5, 19-21: drift left, ping the frame cycle every 4 sub-steps.
///
/// `rest -> first .. last -> rest`; func 5's tick increment is dead in the
/// original and dropped here.
fn flapper(entity: &mut Entity, speed: i32, threshold: u8, rest: u16, first: u16, last: u16) {
    entity.x += speed;
    entity.anim += 1;

    if entity.anim == threshold {
        entity.anim = 0;

        if entity.sprite == rest {
            entity.sprite = first;
        } else if entity.sprite == last {
            entity.sprite = rest;
        } else {
            entity.sprite += 8;
        }
    }
}

/// Function 3: the timed-shot turret (frames 0x338e..0x33ec).
fn turret(entity: &mut Entity, ctx: &mut AiContext) {
    entity.x -= 0x18;
    entity.tick += 1;

    if entity.tick < 0x3c {
        return;
    }

    let t = entity.tick - 0x3c;

    if t == 0x20 {
        ctx.shots.push(Shot {
            sprite: 0x38a6,
            x: entity.x + 0x50,
            y: entity.y + 0x160,
            vx: -0x40,
            vy: 0,
        });
        // TODO: enemy-shot SFX trigger (0xae33).
    }

    entity.anim += 1;

    if entity.anim == 5 {
        entity.anim = 0;

        if entity.sprite == 0x338e {
            entity.sprite = 0x33ac;
        } else if entity.sprite == 0x33ec {
            entity.sprite = 0x338e;
        } else {
            entity.sprite += 8;
        }
    }

    if entity.sprite == 0x33dc {
        entity.x += 0x30;
    }

    if t == 0x32 {
        entity.tick = 0;
        entity.anim = 0;
    }
}

/// Function 4: the carrier pod (frames 0x38b0..0x3926): sine bob in, open,
/// drop a child, retreat fast.
fn carrier_pod(entity: &mut Entity, ctx: &mut AiContext) {
    entity.x -= 0xc;
    entity.tick += 1;

    if entity.tick < 0xc8 {
        // 16-entry bob at file 0xc6ad; the values are 12.4 deltas.
        let at = 0xc6ad + usize::from(entity.tick & 0xf) * 2;
        entity.y += word(ctx.wad, at);
    }

    if entity.tick < 0x32 {
        return;
    }

    if entity.sprite != 0x3926 {
        entity.anim += 1;

        if entity.anim != 8 {
            return;
        }

        entity.anim = 0;

        if entity.sprite == 0x38b0 {
            entity.sprite = 0x38ce;
        } else if entity.sprite != 0x3926 {
            entity.sprite += 8;
        }

        return;
    }

    // Deployed: runs every call while on the final frame; the deploy sound
    // (0xace3, the level's enemy voice) fires once.
    if entity.phase_a & 0xff != 0x11 {
        ctx.sounds.push(SLOT_ENEMY_VOICE);
        entity.phase_a = (entity.phase_a & 0xff00) | 0x11;
    }

    entity.x -= 0x80;

    // The pod streams its child animation every call while deployed; the
    // retreat speed bounds the count, like the original.
    ctx.effects.push(Effect {
        sprite: 0x35e2,
        x: (entity.x >> 4) + 0x12,
        y: (entity.y >> 4) + 4,
        frames: 0xf,
        rate: 1,
        step: 8,
        phase: 0,
        delay: 0,
    });
}

/// `random_aimed_shot` (file 0xc7d4): one `rng(0xe6)` draw per call, fires an
/// aimed 3 px/step shot with p = 1/0xe6, leftward only.
fn random_aimed_shot(entity: &Entity, ctx: &mut AiContext) {
    if ctx.rng.next(0xe6) != 1 {
        return;
    }

    let sx = entity.x + 0x50;
    let sy = entity.y + 0x80;

    if let Some((vx, vy)) = aim_at_player(ctx, sx >> 4, sy >> 4, 3)
        && vx <= 0
    {
        ctx.shots.push(Shot {
            sprite: 0x38a6,
            x: sx,
            y: sy,
            vx,
            vy,
        });
        // TODO: enemy-shot SFX trigger (0xae33).
    }
}

/// `aim_at_player` (file 0xddc4): velocity toward the player at `speed`
/// px/step, in 12.4. Both call sites pre-target `player + (0x1e, 0xa)`; the
/// helper adds another `0xa` to each axis, and the distance comes from the
/// engine's integer square root.
fn aim_at_player(ctx: &AiContext, sx: i32, sy: i32, speed: i32) -> Option<(i32, i32)> {
    let dx = ctx.player_x + 0x1e + 0xa - sx;
    let dy = ctx.player_y + 0xa + 0xa - sy;
    let dist = (i64::from(dx) * i64::from(dx) + i64::from(dy) * i64::from(dy)).isqrt() as i32;
    let scale = dist / speed;

    if scale == 0 {
        return None;
    }

    Some(((dx << 4) / scale, (dy << 4) / scale))
}

/// `shooter_anim` (file 0xc842): random aimed fire, 7-tick ping-pong over
/// 0x33f4/0x3412..0x343a, and a per-frame x wobble.
fn shooter_anim(entity: &mut Entity, ctx: &mut AiContext) {
    random_aimed_shot(entity, ctx);

    entity.anim += 1;

    if entity.anim == 7 {
        entity.anim = 0;

        if entity.sprite == 0x33f4 {
            entity.sprite = 0x3412;
        } else if entity.sprite == 0x343a {
            entity.sprite = 0x33f4;
        } else {
            entity.sprite += 8;
        }
    }

    // Wobble table at file 0xc7c6, indexed by frame (unsigned compare).
    let index = if entity.sprite <= 0x33f4 {
        0
    } else {
        usize::from(entity.sprite - 0x3412 + 8) >> 2
    };

    entity.x += word(ctx.wad, 0xc7c6 + index);
}

/// Which wave tables an orbiter samples.
enum OrbiterShape {
    /// Function 7: y from table A, x from table B, both at +2/call.
    Circle,
    /// Function 8: both axes from table A, x phase at +4/call (figure-8).
    Lissajous,
}

/// Functions 7/8: the big-ship orbiters (frames 0x392e..0x399c).
fn orbiter(entity: &mut Entity, ctx: &mut AiContext, shape: OrbiterShape) {
    entity.tick += 1;

    // Approach bob (file 0xcc97): table at 0xcbaa, 0x5a words.
    if entity.tick <= 1 {
        entity.phase_a = 0;
    }

    if entity.tick <= 0xc8 {
        let at = 0xcbaa + usize::from(entity.phase_a);
        entity.y += word(ctx.wad, at) << 4;
        entity.phase_a += 2;

        if entity.phase_a >= 0xb4 {
            entity.phase_a -= 0xb4;
        }
    }

    if entity.tick <= 0xc8 {
        entity.x -= 0x18;
        return;
    }

    if entity.tick <= 0xc9 {
        entity.phase_a = 0;
        entity.phase_b = match shape {
            OrbiterShape::Circle => 0x10e,
            OrbiterShape::Lissajous => 0,
        };
        entity.save_y = entity.y;
        entity.save_x = entity.x;
        *ctx.gate += 1;
    }

    let (table_y, table_x) = match shape {
        OrbiterShape::Circle => (0xc8da, 0xca42),
        OrbiterShape::Lissajous => (0xc8da, 0xc8da),
    };

    entity.y += word(ctx.wad, table_y + usize::from(entity.phase_a)) << 4;
    entity.x += word(ctx.wad, table_x + usize::from(entity.phase_b)) << 4;

    entity.phase_a += 2;

    if entity.phase_a >= 0x168 {
        entity.phase_a -= 0x168;
        entity.y = entity.save_y;
        entity.x = entity.save_x;
    }

    entity.phase_b += match shape {
        OrbiterShape::Circle => 2,
        OrbiterShape::Lissajous => 4,
    };

    if entity.phase_b >= 0x168 {
        entity.phase_b -= 0x168;
        entity.x += 0x20;
    }

    // Attack animation, gated by vertical proximity to the player.
    // TODO: the original bypasses the gate while the firing weapon
    // (cs:0xcb5) is 3.
    if entity.sprite == 0x392e && ((entity.y >> 4) - ctx.player_y - 0xc).abs() > 0xf {
        return;
    }

    entity.anim += 1;

    if entity.anim == 4 {
        entity.anim = 0;

        if entity.sprite == 0x392e {
            entity.sprite = 0x394c;
        } else if entity.sprite == 0x399c {
            entity.sprite = 0x392e;
        } else {
            entity.sprite += 8;
            orbiter_frame_patch(entity, ctx);
        }
    }
}

/// The orbiter frame patch (file 0xcc5e): the attack frames carry their own
/// middle collision box, from the 12-entry table at file 0xc892.
///
/// The per-frame claw word goes into the kind's SHARED rest descriptor
/// (table at file 0xc8c2, target cs:0x3942 = descriptor +0x14); the death
/// handler reads it through the kind at death time (0xbe21), so the last
/// orbiter stepped decides every orbiter death's debris that frame.
fn orbiter_frame_patch(entity: &mut Entity, ctx: &mut AiContext) {
    let wad = ctx.wad;
    let index = if entity.sprite == 0x392e {
        0
    } else {
        usize::from((entity.sprite - 0x394c) >> 3) + 1
    };

    let at = 0xc892 + index * 4;

    if wad.len() >= at + 4 {
        entity.hitboxes[1] = [wad[at], wad[at + 1], wad[at + 2], wad[at + 3]];
    }

    // The pose's debris pointer (the claw-word table at file 0xc8c2): the
    // death explosion matches the claw extension.
    let claw = 0xc8c2 + index * 2;

    if wad.len() >= claw + 2 {
        let word = u16::from_le_bytes([wad[claw], wad[claw + 1]]);
        ctx.debris_overrides.insert(entity.kind, word);
    }
}

/// The small-popper animation (file 0xce51): every 2 sub-steps over
/// 0x3a92/0x3ab0..0x3ae0.
fn popper_anim(entity: &mut Entity) {
    entity.anim += 1;

    if entity.anim != 2 {
        return;
    }

    entity.anim = 0;

    if entity.sprite == 0x3a92 {
        entity.sprite = 0x3ab0;
    } else if entity.sprite == 0x3ae0 {
        entity.sprite = 0x3a92;
    } else {
        entity.sprite += 8;
    }
}

/// One step along a baked `{dx, dy}` path (pixel deltas at `table + tick*4`).
fn path_step(entity: &mut Entity, ctx: &AiContext, table: usize, wrap: u16) {
    entity.tick += 1;

    if entity.tick >= wrap {
        entity.tick -= wrap;
    }

    let at = table + usize::from(entity.tick) * 4;
    entity.x += word(ctx.wad, at) << 4;
    entity.y += word(ctx.wad, at + 2) << 4;
}

/// Functions 9-12: path follower with the popper animation.
fn path_popper(entity: &mut Entity, ctx: &mut AiContext, table: usize, wrap: u16) {
    path_step(entity, ctx, table, wrap);
    popper_anim(entity);
}

/// Functions 14-17: path follower with the shooter animation/fire.
fn path_shooter(entity: &mut Entity, ctx: &mut AiContext, table: usize, wrap: u16) {
    path_step(entity, ctx, table, wrap);
    shooter_anim(entity, ctx);
}

/// Function 18: the two-gun bomber (frames 0x39a4..0x3a82).
fn bomber(entity: &mut Entity, ctx: &mut AiContext) {
    entity.x -= 8;
    entity.tick += 1;

    if entity.tick <= 0xbe {
        return;
    }

    if entity.tick > 0x12c {
        entity.x += 0x96 - i32::from(entity.tick >> 1);
    }

    entity.anim += 1;

    if entity.anim != 3 {
        return;
    }

    entity.anim = 0;

    if entity.sprite == 0x39a4 {
        entity.sprite = 0x39c2;
        return;
    }

    if entity.sprite == 0x3a02 {
        ctx.shots.push(Shot {
            sprite: 0x38a6,
            x: entity.x + 0x100,
            y: entity.y + 0x80,
            vx: -0x40,
            vy: 0,
        });
    }

    if entity.sprite == 0x3a2a {
        ctx.shots.push(Shot {
            sprite: 0x38a6,
            x: entity.x + 0x100,
            y: entity.y + 0x150,
            vx: -0x40,
            vy: 0,
        });
    }

    if entity.sprite == 0x3a82 {
        entity.sprite = 0x3a02;
    } else {
        entity.sprite += 8;
    }
}

/// The boss's two wave tables: y wave at file 0x2540 (wraps at byte 0xa0), xy
/// wave at 0x25e0 (wraps at 0x104).
const BOSS_WAVE_Y: usize = 0x2540;
const BOSS_WAVE_XY: usize = 0x25e0;

/// Functions 22/23: the level boss (descriptors 0x3ae8.., 0x22-byte stride).
///
/// A tick-keyed phase script over two movement patterns, looping 0xab..0x798
/// until the death flag releases it. The death path needs combat damage to
/// trigger; without it the boss loops its patterns forever.
fn boss(entity: &mut Entity, ctx: &mut AiContext) {
    let boss = &mut *ctx.boss;
    entity.tick += 1;

    let tick = entity.tick;

    if tick <= 0x1 {
        *ctx.gate += 1;
    } else if tick <= 0xaa {
        entity.x -= 0xc;
    } else if tick <= 0xab {
        boss.anchor_x = entity.x;
        boss.anchor_y = entity.y;
        entity.phase_a = 0;
        entity.phase_b = 0x1e;
    } else if tick <= 0x172 {
        boss_circle(entity, ctx.wad, boss);
    } else if tick <= 0x19b {
        entity.phase_a = 0;
        entity.phase_b = 0;
        entity.x += 0x10;
    } else if tick <= 0x321 {
        boss_sweep(entity, ctx.wad, boss);
    } else if tick <= 0x335 {
        entity.phase_a = 0;
        entity.phase_b = 0x1e;
        entity.x -= 0x20;
    } else if tick <= 0x58d {
        boss_circle(entity, ctx.wad, boss);
    } else if tick <= 0x5b5 {
        entity.phase_a = 0;
        entity.phase_b = 0;
        entity.x += 0x10;
    } else if tick <= 0x678 {
        boss_sweep(entity, ctx.wad, boss);
    } else if tick <= 0x679 {
        boss.saved_a = entity.phase_a;
        boss.saved_b = entity.phase_b;
        entity.phase_a = 0;
        entity.phase_b = 0;
        entity.sprite = 0x3ae8;
    } else if tick <= 0x73c {
        entity.phase_a = (entity.phase_a + 2) % 0xa0;
        entity.y = boss.anchor_y + (word(ctx.wad, BOSS_WAVE_Y + usize::from(entity.phase_a)) << 4);
    } else if tick <= 0x742 {
        entity.phase_a = boss.saved_a;
        entity.phase_b = boss.saved_b;
    } else if tick <= 0x783 {
        boss_sweep(entity, ctx.wad, boss);
    } else if tick <= 0x797 {
        entity.phase_a = 0;
        entity.phase_b = 0x1e;
        entity.x -= 0x20;
    } else if tick <= 0x798 {
        if !boss.dying {
            entity.tick = 0xab;
        }
    } else {
        // Exit/death tail.
        if !boss.form2 {
            entity.x += 8 + i32::from(tick) - 0x794;

            if entity.x >= 0x11c0 {
                *ctx.gate = 0;
            }
        } else if tick > 0x810 {
            boss.dying = false;
            entity.health = -1; // health = 0xffff: remove on the next update
        }
    }

    // Every call: the hitbox tracks the current frame's descriptor.
    entity.hitboxes = descriptor_hitboxes(ctx.wad, 0x29f0, entity.sprite);

    // Every call past the fly-in: fire an aimed pair every 0x3c sub-steps.
    if tick >= 0xaa {
        boss.fire_timer -= 1;

        if boss.fire_timer <= 0 {
            boss.fire_timer = 0x3c;
            boss_fire(entity, ctx);
        }
    }

    // Death trigger: hold health at 4000 and start the exit script.
    if entity.health <= 0x1388 && entity.health > 0 {
        entity.health = 0xfa0;
        ctx.boss.dying = true;
        ctx.boss.explosion_timer -= 1;

        if ctx.boss.explosion_timer <= 0 {
            boss_explosion(entity, ctx);
        }
    }
}

/// The boss explosion spawner (file 0xd23b): a staggered burst at a random
/// offset, retimed from the PRNG (draw order 0x14, 0x28, 0x1e).
fn boss_explosion(entity: &Entity, ctx: &mut AiContext) {
    *ctx.boss_explosion = Some(BossExplosionSound::AsteroidPair {
        form2: ctx.boss.form2,
    });

    let mut timer = i32::from(ctx.rng.next(0x14));

    if !ctx.boss.form2 {
        timer += 0xf;
    }

    ctx.boss.explosion_timer = timer;
    let dx = i32::from(ctx.rng.next(0x28)) + 0xf;
    let dy = i32::from(ctx.rng.next(0x1e)) + 0xf;

    ctx.effects.push(Effect {
        sprite: if ctx.boss.form2 { 0x3442 } else { 0x34da },
        x: (entity.x >> 4) + dx,
        y: (entity.y >> 4) + dy,
        frames: 9,
        rate: 3,
        step: 8,
        phase: 0,
        delay: 0,
    });
}

/// Boss pattern 1 (`circle`): y from the 0x234-segment wave, x at 1/8 the
/// amplitude.
fn boss_circle(entity: &mut Entity, wad: &[u8], boss: &BossState) {
    entity.phase_a = (entity.phase_a + 2) % 0xa0;
    entity.y = boss.anchor_y + (word(wad, BOSS_WAVE_Y + usize::from(entity.phase_a)) << 4);
    entity.phase_b = (entity.phase_b + 6) % 0xa0;
    entity.x = boss.anchor_x + (word(wad, BOSS_WAVE_Y + usize::from(entity.phase_b)) << 1);
}

/// Boss pattern 2 (`sweep`): the 0x23e-segment wave drives both axes and the
/// frame index.
fn boss_sweep(entity: &mut Entity, wad: &[u8], boss: &BossState) {
    entity.phase_a = (entity.phase_a + 2) % 0x104;
    entity.y = boss.anchor_y - (word(wad, BOSS_WAVE_XY + usize::from(entity.phase_a)) << 4);

    let xb = (entity.phase_a + 0x40) % 0x104;
    entity.x = boss.anchor_x + (word(wad, BOSS_WAVE_XY + usize::from(xb)) << 5) - 0x640;

    let fb = ((entity.phase_a + 0x8e) % 0x104) >> 2;
    entity.sprite = 0x3ae8 + 0x22 * fb;
}

/// The boss's double aimed shot (file 0xd177): both muzzle points from the
/// current descriptor's +0x1e..0x21 bytes, 4 px/step, unconditional.
fn boss_fire(entity: &Entity, ctx: &mut AiContext) {
    if entity.tick > 0x798 {
        return;
    }

    let descriptor = usize::from(entity.sprite) + 0x29f0;

    for muzzle in [descriptor + 0x1e, descriptor + 0x20] {
        if ctx.wad.len() < muzzle + 2 {
            continue;
        }

        let sx = entity.x + (i32::from(ctx.wad[muzzle]) << 4);
        let sy = entity.y + (i32::from(ctx.wad[muzzle + 1]) << 4);

        if let Some((vx, vy)) = aim_at_player(ctx, sx >> 4, sy >> 4, 4) {
            ctx.shots.push(Shot {
                sprite: 0x38a6,
                x: sx,
                y: sy,
                vx,
                vy,
            });
            // TODO: enemy-shot SFX trigger (0xae33).
        }
    }
}

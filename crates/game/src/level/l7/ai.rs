//! LEVEL_7's 50 mode-0 enemy AI functions, transcribed from the disassembly.
//!
//! See `reference/enemy-ai.md` (pointer table at file `0x10f8b`).
//!
//! Same engine as L1 with a different AI set. No enemy aims (the aimed-fire
//! helper is dead code); the boss is a five-entity composite lava serpent at
//! one shared, wobbling anchor with min-shared health: the controller (arg
//! 4) owns the pattern clock and fires the ring/spiral volleys off the same
//! radial table the smart bomb uses, and the four body parts grow and bite.
//! Most frames are full 0x1e-byte descriptors with per-frame hitboxes,
//! refreshed every sub-step.

use super::{
    DART, DRAGONFLY_L, DRAGONFLY_R, DRONE_L, DRONE_R, EXTRA_LIFE, FOUNTAIN, INVINCIBILITY,
    SMART_BOMB, TRANSPORT, TWIN_GUN, WEAPON_UPGRADE,
};
use crate::level::ai_common::{pickup, word};
use crate::level::prng::EngineRng;
use crate::spawns::{AiSounds, BossExplosionSound, Effect, Entity, Shot, descriptor_hitboxes};

/// LEVEL_7's cs-pointer to file-offset base.
const CS_BASE: usize = 0x51e0;

/// The boss parts' bite sample (the per-level slot 8, trigger id 9).
const SLOT_BITE: usize = 8;

/// Per-step context the AI functions read and write besides the entity.
pub(crate) struct AiContext<'a> {
    pub wad: &'a [u8],
    pub rng: &'a mut EngineRng,
    /// Player y in pixels (the bite-row proximity check; no L7 enemy aims,
    /// so x goes unread).
    pub player_y: i32,
    pub shots: &'a mut Vec<Shot>,
    pub effects: &'a mut Vec<Effect>,
    pub boss: &'a mut BossState,
    /// The boss/snake gate counter (`cs:0x2ed4`).
    pub gate: &'a mut u8,
    /// A boss smoke burst fired this step.
    pub boss_explosion: &'a mut Option<BossExplosionSound>,
    /// Sample slots the AI triggered this step (event channel).
    pub sounds: &'a mut AiSounds,
}

/// The composite boss's shared globals (`cs:0xcc3..0xcdd`).
///
/// Owned by the controller (arg 4) and read by the body parts. File-image
/// inits in the `Default`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct BossState {
    /// The shared anchor (`cs:0xcc3`/`0xcc5`, 12.4; starts at 288, 20 px).
    anchor_x: i32,
    anchor_y: i32,
    /// The wobble outputs all five parts position from (`cs:0xcc7`/`0xcc9`).
    wobble_x: i32,
    wobble_y: i32,
    /// The master tick (`cs:0xccb`) and the pattern clock (`cs:0xccd`).
    master_tick: u16,
    pattern_clock: u16,
    /// The parts' animation gate (`cs:0xccf`); they advance on 0.
    anim_gate: u8,
    /// The spiral's phase (`cs:0xcd0`) and its every-3rd divider (`cs:0xcd2`).
    spiral_phase: u16,
    spiral_divider: u8,
    /// The min-shared health pool (`cs:0xcd3`); the death handler zeroes it
    /// to cascade the remaining parts.
    pub(crate) shared_health: i32,
    /// Smoke burst delay (`cs:0xcd9`, init 0x14) and offsets.
    smoke_delay: i32,
    smoke_dx: i32,
    smoke_dy: i32,
    /// The wobble phases (`cs:0xcdb`/`0xcdd`).
    wobble_phase_x: usize,
    wobble_phase_y: usize,
}

impl Default for BossState {
    fn default() -> Self {
        Self {
            anchor_x: 0x1200,
            anchor_y: 0x140,
            wobble_x: 0,
            wobble_y: 0,
            master_tick: 0,
            pattern_clock: 0,
            anim_gate: 0,
            spiral_phase: 0,
            spiral_divider: 0,
            shared_health: 0x7d00,
            smoke_delay: 0x14,
            smoke_dx: 0,
            smoke_dy: 0,
            wobble_phase_x: 0,
            wobble_phase_y: 0,
        }
    }
}

impl BossState {
    /// Writes the composite boss's shared globals into the save block.
    ///
    /// `base` is the block's first runtime offset; the cluster runs
    /// `cs:0xcc3..0xcdd`. `shared_health` also self-heals from the part
    /// records on the first tick, but the original saves it here too.
    pub(crate) fn save_into(&self, block: &mut [u8], base: usize) {
        fn put_word(block: &mut [u8], at: usize, value: u16) {
            block[at..at + 2].copy_from_slice(&value.to_le_bytes());
        }

        put_word(block, 0xCC3 - base, self.anchor_x as u16);
        put_word(block, 0xCC5 - base, self.anchor_y as u16);
        put_word(block, 0xCC7 - base, self.wobble_x as u16);
        put_word(block, 0xCC9 - base, self.wobble_y as u16);
        put_word(block, 0xCCB - base, self.master_tick);
        put_word(block, 0xCCD - base, self.pattern_clock);
        block[0xCCF - base] = self.anim_gate;
        put_word(block, 0xCD0 - base, self.spiral_phase);
        block[0xCD2 - base] = self.spiral_divider;
        put_word(block, 0xCD3 - base, self.shared_health as u16);
        put_word(block, 0xCD5 - base, self.smoke_dx as u16);
        put_word(block, 0xCD7 - base, self.smoke_dy as u16);
        put_word(block, 0xCD9 - base, self.smoke_delay as u16);
        put_word(block, 0xCDB - base, self.wobble_phase_x as u16);
        put_word(block, 0xCDD - base, self.wobble_phase_y as u16);
    }

    /// Reads the composite boss's shared globals back from the save block.
    pub(crate) fn restore_from(block: &[u8], base: usize) -> Self {
        let word = |at: usize| u16::from_le_bytes([block[at - base], block[at - base + 1]]);

        Self {
            anchor_x: i32::from(word(0xCC3) as i16),
            anchor_y: i32::from(word(0xCC5) as i16),
            wobble_x: i32::from(word(0xCC7) as i16),
            wobble_y: i32::from(word(0xCC9) as i16),
            master_tick: word(0xCCB),
            pattern_clock: word(0xCCD),
            anim_gate: block[0xCCF - base],
            spiral_phase: word(0xCD0),
            spiral_divider: block[0xCD2 - base],
            shared_health: i32::from(word(0xCD3) as i16),
            smoke_delay: i32::from(word(0xCD9) as i16),
            smoke_dx: i32::from(word(0xCD5) as i16),
            smoke_dy: i32::from(word(0xCD7) as i16),
            wobble_phase_x: usize::from(word(0xCDB)),
            wobble_phase_y: usize::from(word(0xCDD)),
        }
    }
}

/// Re-copies the current frame's hitboxes (file `0x1020c`).
fn refresh_hitboxes(entity: &mut Entity, wad: &[u8]) {
    entity.hitboxes = descriptor_hitboxes(wad, CS_BASE, entity.sprite);
}

/// The boss/snake x wobble (file `0x102e3`): 0x78 words, byte-indexed, wrap 0xf0.
const WOBBLE_A: usize = 0x102e3;

/// The boss/snake y wobble (file `0x103d3`): 0x64 words, wrap 0xc8.
const WOBBLE_B: usize = 0x103d3;

/// The dragonflies' morph frame and x-adjust tables.
///
/// Files `0x10d31` and `0x10d49`.
const MORPH_FRAMES: usize = 0x10d31;
const MORPH_XADJ: usize = 0x10d49;

/// The radial velocity table (file `0x80cb`): 32 `{vx, vy}` pairs.
///
/// The same data as L1's smart-bomb ellipse. The boss fires it at half speed.
const RADIAL: usize = 0x80cb;

/// The leftward path segments (funcs 14..23 and 34..43, in arg order).
const LEFT_SEGMENTS: [usize; 10] = [
    0x38, 0x5e, 0x90, 0xc2, 0xf4, 0x126, 0x171, 0x1bc, 0x1ee, 0x22a,
];

/// The rightward path segments (funcs 24..33).
const RIGHT_SEGMENTS: [usize; 10] = [
    0x25c, 0x28e, 0x2c8, 0x313, 0x34d, 0x384, 0x3bb, 0x3f2, 0x43d, 0x47c,
];

/// A path segment's file offset (`segment * 16 + 0x200`).
fn segment_base(segment: usize) -> usize {
    segment * 16 + 0x200
}

/// Runs AI function `arg` for one sub-step.
pub(crate) fn step(entity: &mut Entity, ctx: &mut AiContext) {
    match entity.arg {
        0 => pickup(entity, WEAPON_UPGRADE, 0x42ef),
        1 => pickup(entity, SMART_BOMB, 0x4213),
        2 => pickup(entity, INVINCIBILITY, 0x4289),
        3 => pickup(entity, EXTRA_LIFE, 0x439d),
        4 => boss_controller(entity, ctx),
        5 => boss_part(entity, ctx, PART_2),
        6 => boss_part(entity, ctx, PART_3),
        7 => boss_part(entity, ctx, PART_4),
        8 => boss_part(entity, ctx, PART_5),
        9 => fountain(entity, ctx, FOUNTAIN, 0x471f, 0x4797, 0x47d3),
        10 => fountain(entity, ctx, 0x47f1, 0x4887, 0x48ff, 0x493b),
        11 => dart(entity),
        12 => {
            entity.x -= 0x20;
            anim_small_left(entity, ctx.wad);
        }
        13 => {
            entity.x += 0x20;
            anim_small_right(entity, ctx.wad);
        }
        14..=23 => {
            let segment = LEFT_SEGMENTS[usize::from(entity.arg) - 14];
            anim_small_left(entity, ctx.wad);
            path_add(entity, ctx.wad, segment);
            // The left-edge snap forces the cull (arg 14 snaps at -40 px,
            // the rest at -30).
            let limit = if entity.arg == 14 { -0x280 } else { -0x1e0 };

            if entity.x <= limit {
                entity.x = -0x640;
            }
        }
        24..=33 => {
            let segment = RIGHT_SEGMENTS[usize::from(entity.arg) - 24];
            anim_small_right(entity, ctx.wad);
            path_add(entity, ctx.wad, segment);

            if entity.x >= 0x1200 {
                entity.x = 0x1900;
            }
        }
        34..=43 => directional(entity, ctx.wad, LEFT_SEGMENTS[usize::from(entity.arg) - 34]),
        44 => entity.x -= 0x3c,
        45 => dragonfly(entity, ctx.wad, Facing::Leftward, false),
        46 => dragonfly(entity, ctx.wad, Facing::Leftward, true),
        47 => dragonfly(entity, ctx.wad, Facing::Rightward, false),
        48 => dragonfly(entity, ctx.wad, Facing::Rightward, true),
        49 => snake(entity, ctx),
        _ => {}
    }
}

/// The boss controller and head (func 4).
///
/// Owns the master tick, the pattern clock with the ring/spiral volleys, the
/// anchor and its wobble, and the smoke bursts. The head never animates.
fn boss_controller(entity: &mut Entity, ctx: &mut AiContext) {
    ctx.boss.master_tick += 1;
    ctx.boss.pattern_clock += 1;

    if ctx.boss.pattern_clock == 0x258 {
        ring_volley(ctx);
    } else if ctx.boss.pattern_clock >= 0x384 {
        boss_spiral(ctx);
    }

    ctx.boss.anim_gate += 1;

    if ctx.boss.anim_gate >= 3 {
        ctx.boss.anim_gate = 0;
    }

    if ctx.boss.master_tick <= 0xe6 {
        ctx.boss.anchor_x -= 0x10;

        if ctx.boss.master_tick == 0xc8 {
            *ctx.gate += 1;
        }

        entity.health = 0x7d00;
    } else {
        ctx.boss.wobble_phase_x = (ctx.boss.wobble_phase_x + 2) % 0xf0;
        ctx.boss.wobble_x = -(word(ctx.wad, WOBBLE_A + ctx.boss.wobble_phase_x) << 4);
        ctx.boss.wobble_phase_y = (ctx.boss.wobble_phase_y + 2) % 0xc8;
        ctx.boss.wobble_y = word(ctx.wad, WOBBLE_B + ctx.boss.wobble_phase_y) << 3;
    }

    boss_tail(entity, ctx);
    boss_smoke(entity, ctx);
}

/// The shared per-part tail.
///
/// Position from the anchor plus wobble, then the health min-share across the
/// five parts.
fn boss_tail(entity: &mut Entity, ctx: &mut AiContext) {
    entity.x = ctx.boss.anchor_x + ctx.boss.wobble_x;
    entity.y = ctx.boss.anchor_y + ctx.boss.wobble_y;

    if entity.health >= ctx.boss.shared_health {
        entity.health = ctx.boss.shared_health;
    } else {
        ctx.boss.shared_health = entity.health;
    }

    refresh_hitboxes(entity, ctx.wad);
}

/// A body part's frame schedule.
struct PartData {
    /// Hold until this master tick (invulnerable, health pinned).
    hold: u16,
    /// The grown rest frame the part stops at.
    cap: u16,
    /// The bite cycle's last frame (`None`: the part never bites).
    last: Option<u16>,
    /// The bite row's offset from the anchor row, in pixels.
    row_offset: i32,
}

const PART_2: PartData = PartData {
    hold: 0xc9,
    cap: 0x4e7f,
    last: Some(0x505f),
    row_offset: 0x3c,
};
const PART_3: PartData = PartData {
    hold: 0x12c,
    cap: 0x51a9,
    last: Some(0x5389),
    row_offset: 0,
};
const PART_4: PartData = PartData {
    hold: 0x12c,
    cap: 0x554b,
    last: Some(0x572b),
    row_offset: 0x78,
};
const PART_5: PartData = PartData {
    hold: 0xfa,
    cap: 0x5875,
    last: None,
    row_offset: 0,
};

/// The four body parts (funcs 5..8): hold, grow, then bite.
///
/// Hold, grow one frame per anim-gate pass, then bite when the player's row
/// lines up (parts 2..4; the tail just freezes grown).
fn boss_part(entity: &mut Entity, ctx: &mut AiContext, part: PartData) {
    if ctx.boss.master_tick <= part.hold {
        if ctx.boss.master_tick == 0xc8 {
            *ctx.gate += 1;
        }

        entity.health = 0x7d00;
    } else if ctx.boss.master_tick <= 0x190 {
        if ctx.boss.anim_gate == 0 && entity.sprite < part.cap {
            entity.sprite += 0x1e;
        }
    } else if let Some(last) = part.last
        && ctx.boss.anim_gate == 0
    {
        if entity.sprite > part.cap {
            advance_bite(entity, part.cap, last);
        } else if entity.sprite == part.cap {
            let row = (ctx.boss.anchor_y >> 4) + part.row_offset;

            if (ctx.player_y + 0x16 - row).abs() <= 0x14 {
                ctx.sounds.push(SLOT_BITE);
                advance_bite(entity, part.cap, last);
            }
        }
    }

    boss_tail(entity, ctx);
}

/// One bite-cycle frame step: wraps from the last frame back to the rest.
fn advance_bite(entity: &mut Entity, cap: u16, last: u16) {
    entity.sprite = if entity.sprite >= last {
        cap
    } else {
        entity.sprite + 0x1e
    };
}

/// The boss's smoke bursts below 5000 shared health (file `0x1049b`).
///
/// The controller only.
fn boss_smoke(entity: &mut Entity, ctx: &mut AiContext) {
    if entity.health > 0x1388 {
        return;
    }

    let state = &mut ctx.boss;
    state.smoke_delay -= 1;

    if state.smoke_delay != 0 {
        return;
    }

    *ctx.boss_explosion = Some(BossExplosionSound::Explosion);
    state.smoke_delay = i32::from(ctx.rng.next(0x14)) + 1;
    state.smoke_dx = i32::from(ctx.rng.next(0x3c)) + 0x46;
    state.smoke_dy = i32::from(ctx.rng.next(0x3c)) + 0xf;

    ctx.effects.push(Effect {
        sprite: 0x4465,
        x: (entity.x >> 4) + state.smoke_dx,
        y: (entity.y >> 4) + state.smoke_dy,
        frames: 9,
        rate: 3,
        step: 8,
        phase: 0,
        delay: 0,
    });
}

/// The 16-shot ring from the boss mouth (file `0x105c1`).
///
/// Fires at pattern clock 0x258 exactly.
fn ring_volley(ctx: &mut AiContext) {
    for i in 0..0x10usize {
        let pair = RADIAL + i * 8;
        ctx.shots.push(Shot {
            sprite: 0x3f41,
            x: ctx.boss.anchor_x + 0x7d0 + ctx.boss.wobble_x,
            y: ctx.boss.anchor_y + 0x3e0 + ctx.boss.wobble_y,
            vx: word(ctx.wad, pair) >> 1,
            vy: word(ctx.wad, pair + 2) >> 1,
        });
    }
}

/// The 64-shot spiral (file `0x10552`).
///
/// One shot every 3rd sub-step, stepping every other radial pair; the pattern
/// clock restarts at 0x12c when the sweep completes.
fn boss_spiral(ctx: &mut AiContext) {
    ctx.boss.spiral_divider += 1;

    if ctx.boss.spiral_divider < 3 {
        return;
    }

    ctx.boss.spiral_divider = 0;
    let index = usize::from(ctx.boss.spiral_phase & 0x7f);
    ctx.shots.push(Shot {
        sprite: 0x3f37,
        x: ctx.boss.anchor_x + 0x7b0 + ctx.boss.wobble_x,
        y: ctx.boss.anchor_y + 0x3c0 + ctx.boss.wobble_y,
        vx: word(ctx.wad, RADIAL + index) >> 1,
        vy: word(ctx.wad, RADIAL + index + 2) >> 1,
    });
    ctx.boss.spiral_phase += 8;

    if ctx.boss.spiral_phase >= 0x200 {
        ctx.boss.pattern_clock = 0x12c;
        ctx.boss.spiral_phase = 0;
        ctx.boss.spiral_divider = 0;
    }
}

/// The lava fountains (funcs 9/10): scroll-locked obstacles.
///
/// Bubble low for 100 ticks, then erupt and loop their tall frames.
fn fountain(
    entity: &mut Entity,
    ctx: &mut AiContext,
    rest: u16,
    snap: u16,
    loop_start: u16,
    loop_end: u16,
) {
    if *ctx.gate == 0 {
        entity.x -= 0x10;
    }

    entity.tick += 1;
    entity.anim += 1;

    if entity.anim == 4 {
        entity.anim = 0;

        if entity.sprite >= snap && entity.tick < 0x64 {
            entity.sprite = rest;
        } else if entity.sprite == loop_end {
            entity.sprite = loop_start;
        } else {
            entity.sprite += 0x1e;
        }
    }

    refresh_hitboxes(entity, ctx.wad);
}

/// Func 11: the fast dart (8-byte cycle frames, hitboxes from spawn).
fn dart(entity: &mut Entity) {
    entity.x -= 0x50;
    entity.tick += 1;
    entity.anim += 1;

    if entity.anim == 4 {
        entity.anim = 0;
        entity.sprite = match entity.sprite {
            DART => 0x4ca5,
            0x4cb5 => DART,
            other => other + 8,
        };
    }
}

/// The small leftward enemy's 7-frame cycle (file `0x10a12`).
///
/// Also increments the tick the path followers index with.
fn anim_small_left(entity: &mut Entity, wad: &[u8]) {
    entity.tick += 1;
    entity.anim += 1;

    if entity.anim == 5 {
        entity.anim = 0;
        entity.sprite = if entity.sprite == 0x4c69 {
            DRONE_L
        } else {
            entity.sprite + 0x1e
        };
    }

    refresh_hitboxes(entity, wad);
}

/// The small rightward enemy's cycle (file `0x10a37`).
///
/// Frames `0x4aa7..0x4b79`.
fn anim_small_right(entity: &mut Entity, wad: &[u8]) {
    entity.tick += 1;
    entity.anim += 1;

    if entity.anim == 5 {
        entity.anim = 0;
        entity.sprite = if entity.sprite == 0x4b79 {
            DRONE_R
        } else {
            entity.sprite + 0x1e
        };
    }

    refresh_hitboxes(entity, wad);
}

/// Adds the current tick's path entry (4-byte `{dx, dy}` pairs, no wrap).
fn path_add(entity: &mut Entity, wad: &[u8], segment: usize) {
    let at = segment_base(segment) + usize::from(entity.tick) * 4;
    entity.x += word(wad, at) << 4;
    entity.y += word(wad, at + 2) << 4;
}

/// Funcs 34..43: the three-pose directional flyer.
///
/// The pose switches when two consecutive path entries agree on the dy sign.
fn directional(entity: &mut Entity, wad: &[u8], segment: usize) {
    entity.tick += 1;
    let at = segment_base(segment) + usize::from(entity.tick) * 4;
    entity.x += word(wad, at) << 4;
    let dy = word(wad, at + 2) << 4;
    entity.y += dy;
    let prev = word(wad, at - 2);

    if dy == 0 {
        if prev == 0 {
            entity.sprite = TRANSPORT;
        }
    } else if dy > 0 {
        if prev > 0 {
            entity.sprite = 0x4a89;
        }
    } else if prev < 0 {
        entity.sprite = 0x4a4d;
    }

    refresh_hitboxes(entity, wad);

    if entity.x <= -0x280 {
        entity.x = -0x640;
    }
}

/// Which way a dragonfly enters.
enum Facing {
    /// Funcs 45/46: enter from the right going left, morph, exit right.
    Leftward,
    /// Funcs 47/48: enter from the left going right, morph, exit left.
    Rightward,
}

/// Funcs 45..48: the turn-around dragonflies.
///
/// Fly across, morph to the opposite-facing sprite over 20 sub-steps (with the
/// x-adjust each step), then exit the way they came. The drift variants sink 1
/// px per sub-step during the morph.
fn dragonfly(entity: &mut Entity, wad: &[u8], facing: Facing, drift: bool) {
    entity.tick += 1;
    let (speed, frames_base) = match facing {
        Facing::Leftward => (-0x28, MORPH_FRAMES),
        Facing::Rightward => (0x28, MORPH_FRAMES + 0xc),
    };
    entity.x += speed;

    if entity.tick >= 0x5a {
        entity.x -= speed;

        if entity.tick < 0x6e {
            let index = usize::from(entity.tick - 0x5a) >> 2;
            entity.sprite = word(wad, frames_base + index * 2) as u16;
            entity.x -= word(wad, MORPH_XADJ + index * 2);

            if drift {
                entity.y += 0x10;
            }
        } else {
            match facing {
                Facing::Leftward => {
                    entity.sprite = DRAGONFLY_R;
                    entity.x += 0x28;
                }
                Facing::Rightward => {
                    entity.sprite = DRAGONFLY_L;
                    entity.x -= 0x28;
                }
            }
        }
    }

    refresh_hitboxes(entity, wad);
}

/// Func 49: the snake turret.
///
/// Approaches, anchors and raises the gate, then wobbles in place on the boss
/// tables, firing 6 px/step shots from alternating muzzles in bursts with quiet
/// periods.
///
/// Field mapping: `phase_a`/`phase_b` = the anchor x/y, `save_y`/`save_x` = the
/// wobble phases, `counter` = the burst counter (the original's word at entity
/// +0x2a).
fn snake(entity: &mut Entity, ctx: &mut AiContext) {
    entity.tick += 1;

    if entity.tick == 1 {
        entity.phase_a = 0;
        entity.phase_b = 0;
        entity.save_y = 0;
        entity.save_x = 0;
        entity.counter = 0;
    }

    entity.x -= 0x18;

    if entity.tick < 0x64 {
        return;
    }

    entity.x += 0x18;

    if entity.tick == 0x64 {
        entity.phase_a = entity.x as u16;
        entity.phase_b = entity.y as u16;
        entity.counter = 0;
        *ctx.gate += 1;
    }

    entity.save_y = (entity.save_y + 2) % 0xf0;
    entity.x = i32::from(entity.phase_a) - (word(ctx.wad, WOBBLE_A + entity.save_y as usize) << 4);
    entity.save_x = (entity.save_x + 2) % 0xc8;
    entity.y = i32::from(entity.phase_b) + (word(ctx.wad, WOBBLE_B + entity.save_x as usize) << 3);
    entity.counter += 1;

    if entity.counter <= 0x78 {
        return;
    }

    entity.anim += 1;

    if entity.anim != 5 {
        return;
    }

    entity.anim = 0;

    if entity.sprite == TWIN_GUN {
        entity.sprite = 0x4977;
    } else if entity.sprite == 0x4a27 {
        entity.sprite = 0x49cf;

        if entity.counter >= 0x190 {
            entity.counter = 0;
            entity.sprite = TWIN_GUN;
        }
    } else {
        entity.sprite += 8;
    }

    muzzle_fire(entity, ctx);
}

/// The snake's muzzle shots (file `0x10e1c`).
///
/// The firing frames alternate the upper and lower muzzles.
fn muzzle_fire(entity: &mut Entity, ctx: &mut AiContext) {
    let muzzle = match entity.sprite {
        0x49cf | 0x49e7 | 0x49ff | 0x4a17 => Some(0x70),
        0x49d7 | 0x49ef | 0x4a07 | 0x4a1f => Some(0x270),
        _ => None,
    };

    if let Some(dy) = muzzle {
        ctx.shots.push(Shot {
            sprite: 0x3f37,
            x: entity.x + 0x130,
            y: entity.y + dy,
            vx: -0x60,
            vy: 0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boss_globals_round_trip_through_the_save_block() {
        // The save block's first runtime offset (savegame::BLOCK_BASE).
        let base = 0xCB4;
        let boss = BossState {
            anchor_x: 0x1200,
            anchor_y: 0x140,
            wobble_x: 0x10,
            wobble_y: -0x10,
            master_tick: 0x80,
            pattern_clock: 0x258,
            anim_gate: 1,
            spiral_phase: 0x120,
            spiral_divider: 2,
            shared_health: 0x7D00,
            smoke_delay: 0x14,
            smoke_dx: 5,
            smoke_dy: 6,
            wobble_phase_x: 0x44,
            wobble_phase_y: 0x33,
        };

        let mut block = vec![0u8; 0x100];
        boss.save_into(&mut block, base);

        assert_eq!(block[0xCCB - base..0xCCD - base], 0x80u16.to_le_bytes());
        assert_eq!(block[0xCCD - base..0xCCF - base], 0x258u16.to_le_bytes());
        assert_eq!(block[0xCCF - base], 1);

        assert_eq!(BossState::restore_from(&block, base), boss);
    }
}

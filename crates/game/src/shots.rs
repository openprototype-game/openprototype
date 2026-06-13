//! Player fire: the cooldown state machine, per-weapon spawn patterns, the
//! shot pool, and the chaingun muzzle flash.
//!
//! Reverse-engineered from `LEVEL_1.WAD` (state machine file `0xb68a`,
//! dispatch `0x9a1d`, spawners `0x96f2..`, update loop `0xc35e`); the spawner
//! constants are identical in all seven WADs, with three exceptions carried
//! per level: the despawn x bound ([`crate::levels::CombatData::shot_x_max`]),
//! L5's missile spawn rows, and the L3/L7 fire var-block shift. Shots live in
//! window/buffer coordinates like the ship, in 1/16-pixel fixed point, and
//! move by their velocity each tick. They despawn outside x in (-32,
//! shot_x_max) and y in (-10, 160), and the pool caps at 95 records (the
//! original's "Too-many-Shots" bound).
//!
//! One fire button, held with auto-repeat: a cooldown counter runs up to the
//! firing weapon's rate (set per shot, level-scaled for the secondaries). The
//! firing weapon re-resolves only while fire is NOT held, freezing it across
//! a burst like the original's resolve gate. Firing restarts the muzzle-flash
//! animation, which only shows while the chaingun is the firing weapon.
//!
//! The plasma weapon is the satellite orbs: they trail the ship, riding its
//! position history (delays 0/2/5/7 ticks) with a bobbing wave sampled at
//! staggered phases and growing amplitude down the trail. Holding fire
//! deploys every charged orb at once -- the fire dispatch re-sets all four
//! orb flags from the charge bar on each held tick (L1 file `0x9b48`,
//! congruent in all 7 WADs), overriding the stage machine at `0xafe2`,
//! which only paces the retract (stage `cs:[0xcde]`); each deployed orb
//! fires an instant bolt every tick. Releasing fire retracts the orbs back
//! to front, one every 2nd tick, and launches the
//! last one forward as a slow orb projectile (`cs:[0xce3]`, the launch shot
//! bypasses the fire-held gate).
//!
//! The missile homes: each fired missile locks the round-robin's next live
//! entity and steers toward it once per tick ([`Weapons::steer_missiles`],
//! file `0xc114`), refacing its sprite to the velocity octant and leaving a
//! trail puff every step. The spawners' sound triggers are reported through
//! [`FireSounds`] and mapped to samples by [`crate::sfx`].

use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::{ActiveWeapon, GameState, Weapon};

use crate::assets::{FireSprites, OverlaySprite};
use crate::playfield;
use crate::spawns::{Effect, Entity};

/// The shot pool's record cap (the original errors past `0x5f`).
const MAX_SHOTS: usize = 95;

/// Despawn bounds in 1/16-pixel window coordinates (update loop `0xc35e`).
/// The x maximum is per level ([`CombatData::shot_x_max`]).
const X_MIN: i32 = -0x200;
const Y_MAX: i32 = 0xa00;
const Y_MIN: i32 = -0xa0;

/// Muzzle-flash animation: the offset steps 8 per tick to `0x30` (6 frames),
/// restarted on every shot.
const FLASH_END: i32 = 0x30;

/// Per-orb plasma bolt collision widths (the dispatch pre-writes them into
/// the staging record before each orb's spawner call; all heights are 15).
const BOLT_WIDTHS: [i32; 4] = [21, 22, 28, 28];

/// The burning beam's extra spawn rows per charge level, on top of the
/// shared `y + 16` (the rate selector at file `0x983b`, all 7 WADs).
const BEAM_CHARGE_DY: [i32; 4] = [3, 3, 2, 0];

/// The orbs' bob phase (`cs:[0xcdf]`) steps 2 bytes per tick, wrapping at the
/// wave's 14 words.
const BOB_WRAP: i32 = 0x1c;
/// Per orb: trail delay in ticks, x/y offsets from the trail position, the
/// bob-wave phase stagger in bytes, and the bob's right-shift (amplitude
/// grows down the trail). From the draw pass at file `0xb952` and the
/// weapon-3 dispatch.
const ORBS: [OrbData; 4] = [
    OrbData {
        delay: 0,
        x: 0x2d,
        y: 0x12,
        stagger: 0,
        shift: 2,
    },
    OrbData {
        delay: 2,
        x: 0x3a,
        y: 0x11,
        stagger: 4,
        shift: 1,
    },
    OrbData {
        delay: 5,
        x: 0x46,
        y: 0x10,
        stagger: 8,
        shift: 0,
    },
    OrbData {
        delay: 7,
        x: 0x55,
        y: 0x10,
        stagger: 10,
        shift: 0,
    },
];
/// Ticks the ship's position history remembers (shift chain at `0xb110`).
const TRAIL: usize = 8;

struct OrbData {
    delay: usize,
    x: i32,
    y: i32,
    stagger: i32,
    shift: u32,
}

/// Which sprite a shot draws, resolved against [`FireSprites`] at render.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ShotKind {
    Chaingun,
    /// Charge level 1..=4 picks the sprite.
    Multishot(usize),
    Burning(usize),
    /// An orb's instant bolt; carries which orb (0..4) fired it, because the
    /// dispatch pre-writes a per-orb collision width.
    PlasmaBolt(usize),
    /// The launched orb itself, flying forward after fire is released.
    PlasmaBall,
    Missile,
    /// One record of the smart bomb's expanding ring: inert (zero size and
    /// damage, the hit test's zero-size pre-check skips it), drawn with the
    /// level's ring-shot sprite.
    BombWave,
}

/// What the fire pass did this tick, for the sound triggers.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct FireSounds {
    /// The weapon that spawned shots this tick (for plasma, every held tick:
    /// its dispatch bypasses the cooldown and re-triggers the hum's guard).
    pub fired: Option<ActiveWeapon>,
    /// The plasma ball launched this tick, ending the hum loop.
    pub launched: bool,
    /// The firing weapon resolved to a different one (the original's per-tick
    /// resolve at file `0xae59` plays the switch sound and restarts the pod
    /// on this change, so switching to an uncharged slot is silent and a
    /// switch while fire is held sounds on release).
    pub switched: bool,
}

/// One live shot, in 1/16-pixel window coordinates.
///
/// `damage` is the shot's remaining damage budget (record word +0xe): hits
/// spend it against enemy health, and overkill pierces through with the
/// remainder (see `crate::combat`).
pub struct Shot {
    kind: ShotKind,
    pub x: i32,
    pub y: i32,
    dx: i32,
    dy: i32,
    pub damage: i32,
    /// The missile's homing lock (record byte `+0xc`): a 1-based index into
    /// the live entities, `0` for none. Steering drops a stale lock.
    target: u16,
    /// The missile's facing octant, `0` = right counting clockwise; picks
    /// the render sprite (the original rewrites the record's descriptor
    /// pointer to `0x32c8 + octant*8`).
    pub octant: usize,
}

impl Shot {
    /// The shot's collision extent in pixels (record bytes +0xa/+0xb), from
    /// the spawner literals.
    pub fn collision_size(&self) -> (i32, i32) {
        match self.kind {
            ShotKind::Chaingun | ShotKind::Missile => (13, 4),
            ShotKind::Multishot(_) => (5, 4),
            ShotKind::Burning(_) => (135, 12),
            // The dispatch pre-writes each orb's bolt width before the
            // spawner runs; the ball's (22, 15) is the spawner's own write
            // at file 0x9924.
            ShotKind::PlasmaBolt(orb) => (BOLT_WIDTHS[orb.min(3)], 15),
            ShotKind::PlasmaBall => (22, 15),
            ShotKind::BombWave => (0, 0),
        }
    }

    /// Whether the damage path skips the hit spark (the original's `0x32c0`
    /// plasma type check at file 0xc0df).
    pub fn is_plasma(&self) -> bool {
        matches!(self.kind, ShotKind::PlasmaBolt(_) | ShotKind::PlasmaBall)
    }

    /// Whether a hit by this shot plays the chaingun impact (`0xad83`).
    pub fn is_chaingun(&self) -> bool {
        matches!(self.kind, ShotKind::Chaingun)
    }

    /// Whether a hit by this shot plays the missile impact (`0xad63`).
    pub fn is_missile(&self) -> bool {
        matches!(self.kind, ShotKind::Missile)
    }

    /// Whether this is a multishot round (sprite below the burning
    /// threshold, sharing the chaingun's spark family).
    pub fn is_multishot(&self) -> bool {
        matches!(self.kind, ShotKind::Multishot(_))
    }
}

/// The missile's per-octant draw offsets in pixels (the shot draw at file
/// `0xbc2d` adds these, 12.4-scaled, before the blit). The record position
/// is the hit-test/trail anchor; these seat the rotated sprite on it.
const MISSILE_DRAW_OFFSETS: [(i32, i32); 8] = [
    (2, 4),
    (3, 6),
    (-2, 7),
    (-5, 6),
    (-9, 3),
    (-9, 1),
    (-2, 0),
    (2, 1),
];

/// One homing step (the locked branch of file `0xc114`).
fn steer(shot: &mut Shot, enemy: &Entity, wad: &[u8], cs_base: usize) {
    let descriptor = usize::from(enemy.kind) + cs_base;
    let diff_x = enemy.x + read_word(wad, descriptor + 0x1a) - shot.x;
    let diff_y = enemy.y + read_word(wad, descriptor + 0x1c) - shot.y;

    // Inverse-squared-distance weighting: the closer the target, the harder
    // the turn. A zero weight means point blank; the lock drops.
    let weight = (diff_x * diff_x + diff_y * diff_y) / 3;

    if weight == 0 {
        shot.target = 0;
        return;
    }

    let new_dx = shot.dx + diff_x * 0x1800 / weight;
    let new_dy = shot.dy + diff_y * 0x1800 / weight;

    // Renormalize to a constant speed of 0x30 (3 px) per step.
    let length = (i64::from(new_dx) * i64::from(new_dx) + i64::from(new_dy) * i64::from(new_dy))
        .isqrt() as i32;
    let scale = length / 3;

    if scale == 0 {
        shot.target = 0;
        return;
    }

    shot.dx = (new_dx << 4) / scale;
    shot.dy = (new_dy << 4) / scale;
    shot.octant = octant(shot.dx, shot.dy);
}

/// Classify a velocity into a facing octant, `0` = right counting clockwise
/// (file `0xc1bd..`). The diagonal band is `major/4 < |minor| <= major`.
fn octant(dx: i32, dy: i32) -> usize {
    match (dx >= 0, dy >= 0) {
        (true, true) => {
            if dy > dx {
                2
            } else if dx / 4 < dy {
                1
            } else {
                0
            }
        }
        (true, false) => {
            if -dy > dx {
                6
            } else if dx / 4 < -dy {
                7
            } else {
                0
            }
        }
        (false, true) => {
            if dy > -dx {
                2
            } else if -dx / 4 < dy {
                3
            } else {
                4
            }
        }
        (false, false) => {
            if -dy > -dx {
                6
            } else if -dx / 4 < -dy {
                5
            } else {
                4
            }
        }
    }
}

/// Reads a sign-extended word from the WAD image (zero out of bounds).
fn read_word(wad: &[u8], at: usize) -> i32 {
    if wad.len() < at + 2 {
        return 0;
    }

    i32::from(i16::from_le_bytes([wad[at], wad[at + 1]]))
}

/// A shot's initial damage budget (the spawners' `+0xe` literals).
fn initial_damage(kind: ShotKind) -> i32 {
    match kind {
        ShotKind::Chaingun => 12,
        ShotKind::Multishot(level) => [15, 18, 20, 22][level],
        ShotKind::Burning(level) => [60, 70, 90, 125][level],
        ShotKind::PlasmaBolt(_) | ShotKind::PlasmaBall => 30,
        ShotKind::Missile => 80,
        ShotKind::BombWave => 0,
    }
}

/// The player's fire state: the cooldown machine, the live shots, and the
/// muzzle flash.
pub struct Weapons {
    /// Cooldown counter (`cs:[0xcc7]`), counting up to `rate`.
    cooldown: u8,
    /// The firing weapon's auto-repeat period (`cs:[0xcc8]`), set per shot.
    rate: u8,
    /// The resolved firing weapon, frozen while fire is held.
    firing: ActiveWeapon,
    /// The missile's alternating spawn-offset toggle (`cs:[0x267e]`).
    missile_toggle: bool,
    /// The missile lock's round-robin counter (`cs:[0x267f]`): each fired
    /// missile takes the current value as its target, then the counter
    /// advances and wraps into `1..=live entities` (`0` while none live).
    missile_target: u16,
    /// Muzzle-flash animation offset (`cs:[0xccb]`), `>= FLASH_END` = idle.
    flash_offset: i32,
    /// How many plasma orbs are deployed (`cs:[0xcde]` stage and the
    /// `cs:[0xcda..0xcdd]` flags).
    orbs: usize,
    /// Whether releasing fire should retract and launch the orbs
    /// (`cs:[0xce2]`, armed while firing plasma).
    launch_armed: bool,
    /// The deploy/retract half-tick divider (`cs:[0xce1]`).
    orb_step_divider: u8,
    /// A launch shot is due (`cs:[0xce3]`); fired on the next tick without
    /// needing fire held.
    launch_pending: bool,
    /// The orbs' bob phase (`cs:[0xcdf]`), a byte offset into the wave.
    bob_phase: i32,
    /// The orbs' animation-frame offset (`cs:[0xcd8]`), over 8.
    orb_anim: usize,
    /// Divider so the orb animation advances every 2nd tick (`cs:[0xcd7]`).
    orb_anim_divider: u8,
    /// The ship's recent positions, newest first (the original's shift chain
    /// over `cs:[0x2644..0x2662]`).
    trail: [(i32, i32); TRAIL],
    /// The orbs' bob wave, from the level WAD.
    bob_wave: Vec<i32>,
    /// The level's despawn x bound ([`CombatData::shot_x_max`]).
    shot_x_max: i32,
    /// The level's missile spawn rows ([`FireData::missile_rows`]).
    missile_rows: (i32, i32),
    /// A smart bomb fired since the last update: the bomb takes the
    /// dispatch slot, so that tick's volley AND its flash/cooldown reset
    /// are skipped (the bomb branch jumps past both, L1 0xb722 -> 0xb753).
    bomb_fired: bool,
    /// The bomb key was pressed while the ring is still in flight: the
    /// dispatch falls through into the volley path (L1 0xb730), firing a
    /// full weapon volley that bypasses the cooldown and the fire gate.
    bypass_volley: bool,
    pub shots: Vec<Shot>,
}

impl Weapons {
    /// `firing` is the initial firing weapon, normally the resolve of the
    /// starting [`GameState`] (so the first tick's re-resolve is a no-op
    /// rather than a spurious switch).
    pub fn new(
        bob_wave: Vec<i32>,
        firing: ActiveWeapon,
        shot_x_max: i32,
        missile_rows: (i32, i32),
    ) -> Self {
        Self {
            cooldown: 0,
            rate: 6,
            firing,
            missile_toggle: false,
            missile_target: 0,
            flash_offset: FLASH_END,
            orbs: 0,
            launch_armed: false,
            orb_step_divider: 0,
            launch_pending: false,
            bob_phase: 0,
            orb_anim: 0,
            orb_anim_divider: 0,
            trail: [(0, 0); TRAIL],
            bob_wave,
            shot_x_max,
            missile_rows,
            bomb_fired: false,
            bypass_volley: false,
            shots: Vec::new(),
        }
    }

    /// Each orb's window position: its trail entry plus its fixed offset and
    /// its staggered, amplitude-shifted sample of the bob wave.
    fn orb_positions(&self) -> [(i32, i32); 4] {
        std::array::from_fn(|orb| {
            let data = &ORBS[orb];
            let (trail_x, trail_y) = self.trail[data.delay];
            let wave_index = ((self.bob_phase + data.stagger) / 2) as usize;
            let bob = self.bob_wave.get(wave_index).copied().unwrap_or(0) >> data.shift;

            (trail_x + data.x, trail_y + data.y + bob)
        })
    }

    /// Advance one logic tick: re-resolve the firing weapon (unless frozen by
    /// held fire), run the cooldown, spawn due shots from the ship at `(x, y)`
    /// with roll frame `roll` (for the barrel offsets), move the live shots,
    /// and advance the flash. Returns what fired, for the sound triggers.
    ///
    /// `enemy_count` is the live entity count, for the missile lock's
    /// round-robin counter.
    pub fn update(
        &mut self,
        fire_held: bool,
        state: &GameState,
        ship: (i32, i32),
        roll: usize,
        barrels: &[(i32, i32)],
        enemy_count: usize,
    ) -> FireSounds {
        let mut switched = false;

        if !fire_held {
            let resolved = state.active_weapon();
            switched = resolved != self.firing;
            self.firing = resolved;
        }

        self.trail.rotate_right(1);
        self.trail[0] = ship;

        self.bob_phase = (self.bob_phase + 2) % BOB_WRAP;
        self.orb_anim_divider += 1;

        if self.orb_anim_divider == 2 {
            self.orb_anim_divider = 0;
            self.orb_anim = (self.orb_anim + 1) % 4;
        }

        if self.flash_offset < FLASH_END {
            self.flash_offset += 8;
        }

        // Move before firing, so a fresh shot renders at its spawn position
        // this frame (the plasma bolts cross the whole window in one tick and
        // would otherwise despawn unseen).
        for shot in &mut self.shots {
            shot.x += shot.dx;
            shot.y += shot.dy;
        }

        self.shots.retain(|shot| {
            shot.x < self.shot_x_max && shot.x > X_MIN && shot.y < Y_MAX && shot.y > Y_MIN
        });

        // The cooldown counts up to the rate and holds there; firing resets
        // it to zero (file 0xb68a). Plasma bypasses the counter entirely.
        let due = if self.cooldown < self.rate {
            self.cooldown += 1;
            self.cooldown >= self.rate
        } else {
            true
        };

        let plasma = self.firing == ActiveWeapon::Selected(Weapon::Plasma);
        let launched = self.step_orbs(fire_held && plasma, state, ship);
        let mut fired = None;

        // The bomb branch takes the dispatch slot for its tick (no volley,
        // no cooldown reset); a bomb press during the ring's flight instead
        // forces a volley past both gates.
        let bomb_took_slot = std::mem::take(&mut self.bomb_fired);
        let force = std::mem::take(&mut self.bypass_volley);

        if !bomb_took_slot && (force || (fire_held && (due || plasma))) {
            self.fire(state, ship, roll, barrels, enemy_count);
            self.cooldown = 0;
            fired = Some(self.firing);
        }

        FireSounds {
            fired,
            launched,
            switched,
        }
    }

    /// The orb deploy/retract machine: while fire is held, the dispatch
    /// re-sets every orb flag from the charge bar (L1 file `0x9b48`), so all
    /// charged orbs are out from the first tick and a drained charge pulls
    /// them back in step; the stage machine (file `0xafe2`) only paces the
    /// retract, one orb every 2nd tick after release, launching the last as
    /// a forward orb projectile. Returns whether the ball launched this
    /// tick.
    fn step_orbs(&mut self, plasma_held: bool, state: &GameState, (x, y): (i32, i32)) -> bool {
        let mut launched = false;

        if self.launch_pending {
            self.launch_pending = false;
            self.spawn(ShotKind::PlasmaBall, x + ORBS[0].x, y + ORBS[0].y, 160, 0);
            launched = true;
        }

        if plasma_held {
            // The held dispatch is the last writer of the orb flags every
            // tick; re-pressing mid-retract snaps back to full deployment.
            self.orbs = charge_index(state, Weapon::Plasma) + 1;
            self.launch_armed = true;
        } else if self.launch_armed {
            self.orb_step_divider += 1;

            if self.orb_step_divider == 2 {
                self.orb_step_divider = 0;

                if self.orbs > 1 {
                    self.orbs -= 1;
                } else {
                    self.orbs = 0;
                    self.launch_armed = false;
                    self.launch_pending = true;
                }
            }
        }

        launched
    }

    /// Spawn the firing weapon's shots from ship position `(x, y)` (window
    /// pixels) and set its auto-repeat rate. Restarts the muzzle flash.
    fn fire(
        &mut self,
        state: &GameState,
        (x, y): (i32, i32),
        roll: usize,
        barrels: &[(i32, i32)],
        enemy_count: usize,
    ) {
        let (barrel_a, barrel_b) = barrels.get(roll).copied().unwrap_or((0, 0));
        self.flash_offset = 0;

        match self.firing {
            ActiveWeapon::Chaingun => {
                self.spawn(ShotKind::Chaingun, x + 48, y + barrel_a + 4, 120, 0);
                self.spawn(ShotKind::Chaingun, x + 48, y + barrel_b + 4, 120, 0);
                self.rate = 6;
            }
            ActiveWeapon::Selected(Weapon::Multishot) => {
                let level = charge_index(state, Weapon::Multishot);
                let kind = ShotKind::Multishot(level);
                self.spawn(kind, x + 48, y + barrel_a + 2, 112, -32);
                self.spawn(kind, x + 48, y + barrel_b + 2, 112, 32);
                self.spawn(kind, x + 51, y + 20, 128, 0);

                if level == 3 {
                    self.spawn(kind, x, y + 25, -128, 12);
                    self.spawn(kind, x, y + 15, -128, -12);
                }

                self.rate = [8, 7, 6, 5][level];
            }
            ActiveWeapon::Selected(Weapon::Burning) => {
                let level = charge_index(state, Weapon::Burning);
                let dy = 16 + BEAM_CHARGE_DY[level];
                self.spawn(ShotKind::Burning(level), x, y + dy, 224, 0);
                self.rate = [19, 18, 17, 16][level];
            }
            ActiveWeapon::Selected(Weapon::Plasma) => {
                // Each deployed orb fires an instant bolt from 10 rows above
                // its position, every tick (plasma bypasses the cooldown).
                let positions = self.orb_positions();

                for (orb, &(x, y)) in positions.iter().take(self.orbs).enumerate() {
                    self.spawn(ShotKind::PlasmaBolt(orb), x, y - 10, 5120, 0);
                }
            }
            ActiveWeapon::Selected(Weapon::Missile) => {
                let level = charge_index(state, Weapon::Missile);
                let (base_dy, alt_dy) = self.missile_rows;
                let dy = if self.missile_toggle { alt_dy } else { 0 };
                self.missile_toggle = !self.missile_toggle;

                // The lock counter pre-increments and wraps into the live
                // entities BEFORE the spawner stores it (inc at file 0x9cc2,
                // 1-based, 0 while nothing lives, ds-image init 0), so the
                // first missile already locks entity 1.
                self.missile_target += 1;

                if enemy_count == 0 {
                    self.missile_target = 0;
                } else {
                    while usize::from(self.missile_target) > enemy_count {
                        self.missile_target -= enemy_count as u16;
                    }
                }

                let target = self.missile_target;
                let before = self.shots.len();
                self.spawn(ShotKind::Missile, x + 35, y + base_dy + dy, 48, 0);

                if self.shots.len() > before
                    && let Some(missile) = self.shots.last_mut()
                {
                    missile.target = target;
                }

                self.rate = [45, 35, 25, 15][level];
            }
        }
    }

    /// The resolved firing weapon (`cs:0xcb5`), frozen while fire is held.
    /// The panel's pod and overlay track this, not the instantaneous
    /// selection: an orb picked up mid-burst doesn't switch the display
    /// until fire is released (the resolve at file 0xae59).
    pub fn firing(&self) -> ActiveWeapon {
        self.firing
    }

    /// The firing weapon's bar hit zero mid-hold: revert to the chaingun
    /// with the original's cooldown (the hit consequence at file 0xc52c).
    pub fn weapon_lost(&mut self) {
        self.firing = ActiveWeapon::Chaingun;
        self.rate = 6;
    }

    /// Spawn the smart bomb's expanding ring (file `0x99b7`): 32 inert wave
    /// records from `(ship + 25, ship + 20)` px, one per velocity in the
    /// level's ellipse table. They fly out as ordinary shots and despawn at
    /// the bounds; the bomb's field damage lands separately, 14 ticks later.
    pub fn smart_bomb(&mut self, (x, y): (i32, i32), wave: &[(i32, i32)]) {
        for &(dx, dy) in wave {
            self.spawn(ShotKind::BombWave, x + 25, y + 20, dx, dy);
        }

        self.bomb_fired = true;
    }

    /// The bomb key landed while the ring is still in flight: the original's
    /// dispatch falls through into the volley path (L1 `0xb730`), so the
    /// next update fires the firing weapon's volley past every gate.
    pub fn force_volley(&mut self) {
        self.bypass_volley = true;
    }

    /// One steering step for every live missile (file `0xc114`, run per
    /// movement sub-step in the original's shot pass, between the hit test
    /// and the velocity add).
    ///
    /// A locked missile accelerates toward its target's center (descriptor
    /// `+0x1a`/`+0x1c` offsets) weighted by inverse squared distance, then
    /// renormalizes to a constant 3 px/step and refaces to the octant of the
    /// new velocity. Locks drop when stale (index past the live entities) or
    /// at point blank. Every missile leaves a trail puff every step, locked
    /// or not.
    pub fn steer_missiles(
        &mut self,
        entities: &[Entity],
        wad: &[u8],
        cs_base: usize,
        trail_sprite: u16,
        effects: &mut Vec<Effect>,
    ) {
        for shot in &mut self.shots {
            if !shot.is_missile() {
                continue;
            }

            if shot.target > 0 {
                if usize::from(shot.target) > entities.len() {
                    shot.target = 0;
                } else {
                    steer(shot, &entities[usize::from(shot.target) - 1], wad, cs_base);
                }
            }

            effects.push(Effect {
                sprite: trail_sprite,
                x: shot.x >> 4,
                y: (shot.y >> 4) + 4,
                frames: 0x12,
                rate: 1,
                step: 8,
                phase: 0,
                delay: 0,
            });
        }
    }

    fn spawn(&mut self, kind: ShotKind, x: i32, y: i32, dx: i32, dy: i32) {
        if self.shots.len() >= MAX_SHOTS {
            return;
        }

        self.shots.push(Shot {
            kind,
            x: x << 4,
            y: y << 4,
            dx,
            dy,
            damage: initial_damage(kind),
            target: 0,
            octant: 0,
        });
    }

    /// Composite the live shots and the plasma orbs (window coordinates, like
    /// the ship; the orbs only show while plasma is the firing weapon, drawn
    /// furthest-back first like the original's `0xb952` pass).
    ///
    /// `show_orbs` is off while the ship is dying: the original's draw block
    /// skips the orbs together with the ship (the `0x46b2` gate at `0xb952`),
    /// while live shots draw in their own ungated pass.
    pub fn render(
        &self,
        sprites: &FireSprites,
        frame: &mut Framebuffer,
        camera: i32,
        show_orbs: bool,
    ) {
        for shot in &self.shots {
            // The missile draws offset from its record position, per facing
            // octant (the shot draw at file 0xbc2d): the record anchors the
            // hit test and the trail, the offsets seat the sprite on them.
            let (sprite, (offset_x, offset_y)) = match shot.kind {
                ShotKind::Chaingun => (&sprites.chaingun, (0, 0)),
                ShotKind::Multishot(level) => (&sprites.multishot[level], (0, 0)),
                ShotKind::Burning(level) => (&sprites.burning[level], (0, 0)),
                ShotKind::PlasmaBolt(_) => (&sprites.plasma_bolt, (0, 0)),
                ShotKind::PlasmaBall => (&sprites.plasma_orbs[1][0], (0, 0)),
                ShotKind::Missile => (
                    &sprites.missile[shot.octant],
                    MISSILE_DRAW_OFFSETS[shot.octant],
                ),
                ShotKind::BombWave => (&sprites.bomb_ring, (0, 0)),
            };

            blit(
                frame,
                sprite,
                (shot.x >> 4) + offset_x,
                (shot.y >> 4) + offset_y - camera,
            );
        }

        if show_orbs && self.firing == ActiveWeapon::Selected(Weapon::Plasma) {
            let positions = self.orb_positions();

            for orb in (0..self.orbs.min(ORBS.len())).rev() {
                let (x, y) = positions[orb];
                blit(
                    frame,
                    &sprites.plasma_orbs[orb][self.orb_anim],
                    x,
                    y - camera,
                );
            }
        }
    }

    /// Composite the muzzle flash over the ship while the chaingun fires:
    /// twice, at the two barrel positions of the current roll frame.
    pub fn render_flash(
        &self,
        sprites: &FireSprites,
        frame: &mut Framebuffer,
        ship: (i32, i32),
        roll: usize,
        barrels: &[(i32, i32)],
        camera: i32,
    ) {
        if self.flash_offset >= FLASH_END || self.firing != ActiveWeapon::Chaingun {
            return;
        }

        let Some(sprite) = sprites.muzzle_flash.get((self.flash_offset / 8) as usize) else {
            return;
        };

        let (barrel_a, barrel_b) = barrels.get(roll).copied().unwrap_or((0, 0));
        let (x, y) = ship;
        blit(frame, sprite, x + 0x2b, y + barrel_a - camera);
        blit(frame, sprite, x + 0x2b, y + barrel_b - camera);
    }
}

/// A weapon's charge level as a 0-based index into the per-level tables.
/// Callers only fire a secondary while it holds charge, so level 0 never
/// reaches here; it clamps defensively.
fn charge_index(state: &GameState, weapon: Weapon) -> usize {
    (state.level(weapon).get().max(1) - 1) as usize
}

fn blit(frame: &mut Framebuffer, sprite: &OverlaySprite, x: i32, y: i32) {
    frame.blit_transparent(&sprite.pixels, sprite.size, playfield::LEFT + x, y);
}

#[cfg(test)]
mod tests {
    use super::*;
    use openprototype_core::{Lives, PerWeapon, SmartBombs, WeaponLevel};

    const BARRELS: [(i32, i32); 27] = [(0, 8); 27];

    fn state(selected: Weapon, level: u8) -> GameState {
        let mut weapons = PerWeapon::splat(WeaponLevel::new(0));
        *weapons.get_mut(selected) = WeaponLevel::new(level);

        GameState {
            score: 0,
            lives: Lives::new(3),
            smart_bombs: SmartBombs::new(3),
            weapons,
            selected,
            invincible_ticks: 0,
            contact_grace_ticks: 0,
        }
    }

    fn run(weapons: &mut Weapons, fire: bool, state: &GameState, ticks: u32) {
        for _ in 0..ticks {
            weapons.update(fire, state, (100, 60), 0, &BARRELS, 0);
        }
    }

    #[test]
    fn the_chaingun_fires_two_barrel_shots_on_its_cooldown() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let state = state(Weapon::Multishot, 0); // empty slot -> chaingun

        run(&mut weapons, true, &state, 6);
        assert_eq!(weapons.shots.len(), 2);

        // The next burst lands after the full rate elapses again.
        run(&mut weapons, true, &state, 5);
        assert_eq!(weapons.shots.len(), 2);
        run(&mut weapons, true, &state, 1);
        assert_eq!(weapons.shots.len(), 4);
    }

    #[test]
    fn shots_move_and_despawn_at_the_window_bounds() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let state = state(Weapon::Multishot, 0);

        run(&mut weapons, true, &state, 6);
        let first_x = weapons.shots[0].x;
        run(&mut weapons, false, &state, 1);
        assert_eq!(weapons.shots[0].x, first_x + 120);

        // 7.5 px/tick from x 148: off the 288-px window within ~19 ticks.
        run(&mut weapons, false, &state, 30);
        assert!(weapons.shots.is_empty());
    }

    #[test]
    fn max_level_multishot_adds_the_two_backward_shots() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let low = state(Weapon::Multishot, 1);
        run(&mut weapons, false, &low, 1);
        run(&mut weapons, true, &low, 8);
        assert_eq!(weapons.shots.len(), 3);

        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let max = state(Weapon::Multishot, 4);
        run(&mut weapons, false, &max, 1);
        run(&mut weapons, true, &max, 8);
        assert_eq!(weapons.shots.len(), 5);
        assert!(weapons.shots.iter().any(|shot| shot.dx < 0));
    }

    #[test]
    fn the_firing_weapon_freezes_while_fire_is_held() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let mut state = state(Weapon::Burning, 2);

        run(&mut weapons, false, &state, 1);
        run(&mut weapons, true, &state, 1);
        assert_eq!(weapons.firing, ActiveWeapon::Selected(Weapon::Burning));

        // Draining the slot mid-burst does not change the firing weapon...
        *state.weapons.get_mut(Weapon::Burning) = WeaponLevel::new(0);
        run(&mut weapons, true, &state, 1);
        assert_eq!(weapons.firing, ActiveWeapon::Selected(Weapon::Burning));

        // ...until fire is released.
        run(&mut weapons, false, &state, 1);
        assert_eq!(weapons.firing, ActiveWeapon::Chaingun);
    }

    #[test]
    fn plasma_deploys_every_charged_orb_at_once() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let state = state(Weapon::Plasma, 3);

        // The first held tick brings out every charged orb, each firing a
        // bolt.
        run(&mut weapons, false, &state, 1);
        run(&mut weapons, true, &state, 1);
        assert_eq!(weapons.orbs, 3);
        assert_eq!(weapons.shots.len(), 3);

        // The bolts cross the window in one tick, so the pool holds exactly
        // one volley while fire is held: plasma bypasses the cooldown.
        run(&mut weapons, true, &state, 10);
        assert_eq!(weapons.orbs, 3);
        assert_eq!(weapons.shots.len(), 3);
    }

    #[test]
    fn a_drained_charge_pulls_deployed_orbs_back_in_step() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let mut state = state(Weapon::Plasma, 3);

        run(&mut weapons, false, &state, 1);
        run(&mut weapons, true, &state, 1);
        assert_eq!(weapons.orbs, 3);

        // The flags re-set from the charge bar every held tick.
        *state.weapons.get_mut(Weapon::Plasma) = WeaponLevel::new(1);
        run(&mut weapons, true, &state, 1);
        assert_eq!(weapons.orbs, 1);
    }

    #[test]
    fn releasing_fire_retracts_the_orbs_and_launches_the_ball() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let state = state(Weapon::Plasma, 2);

        run(&mut weapons, false, &state, 1);
        run(&mut weapons, true, &state, 4);
        assert_eq!(weapons.orbs, 2);

        // Release: one orb retracts every 2nd tick, the last one launches.
        run(&mut weapons, false, &state, 2);
        assert_eq!(weapons.orbs, 1);
        run(&mut weapons, false, &state, 3);
        assert_eq!(weapons.orbs, 0);
        assert_eq!(
            weapons
                .shots
                .iter()
                .filter(|shot| shot.kind == ShotKind::PlasmaBall)
                .count(),
            1
        );

        // The ball flies forward at 10 px/tick and nothing rearms until
        // fire is held again.
        run(&mut weapons, false, &state, 5);
        assert_eq!(
            weapons
                .shots
                .iter()
                .filter(|shot| shot.kind == ShotKind::PlasmaBall)
                .count(),
            1
        );
    }

    #[test]
    fn the_orbs_ride_the_ship_trail() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let state = state(Weapon::Plasma, 4);
        run(&mut weapons, false, &state, 1);

        // Move the ship one pixel right per tick; the orbs' trail entries lag
        // by their per-orb delays.
        for tick in 0..20 {
            weapons.update(true, &state, (100 + tick, 60), 0, &BARRELS, 0);
        }

        let positions = weapons.orb_positions();
        let lead = positions[0].0 - ORBS[0].x;
        assert_eq!(positions[1].0 - ORBS[1].x, lead - 2);
        assert_eq!(positions[2].0 - ORBS[2].x, lead - 5);
        assert_eq!(positions[3].0 - ORBS[3].x, lead - 7);
    }

    #[test]
    fn the_resolve_reports_a_switch_only_when_the_firing_weapon_changes() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let mut state = state(Weapon::Multishot, 2);
        *state.weapons.get_mut(Weapon::Burning) = WeaponLevel::new(2);

        // Chaingun -> multishot reports once, then settles.
        let sounds = weapons.update(false, &state, (100, 60), 0, &BARRELS, 0);
        assert!(sounds.switched);
        let sounds = weapons.update(false, &state, (100, 60), 0, &BARRELS, 0);
        assert!(!sounds.switched);

        // While fire is held the resolve freezes, so changing the selection
        // stays silent until release.
        state.selected = Weapon::Burning;
        let sounds = weapons.update(true, &state, (100, 60), 0, &BARRELS, 0);
        assert!(!sounds.switched);
        let sounds = weapons.update(false, &state, (100, 60), 0, &BARRELS, 0);
        assert!(sounds.switched);

        // Selecting an uncharged slot resolves back to the chaingun: one
        // switch, then silence again.
        state.selected = Weapon::Missile;
        let sounds = weapons.update(false, &state, (100, 60), 0, &BARRELS, 0);
        assert!(sounds.switched);
        let sounds = weapons.update(false, &state, (100, 60), 0, &BARRELS, 0);
        assert!(!sounds.switched);
    }

    #[test]
    fn the_bomb_takes_the_dispatch_slot_for_its_tick() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let state = state(Weapon::Multishot, 2);
        run(&mut weapons, false, &state, 1);
        weapons.cooldown = 100;

        // The bomb fires; that tick's held-fire volley is suppressed and
        // the cooldown is left alone.
        weapons.smart_bomb((100, 60), &[(16, 0)]);
        let ring = weapons.shots.len();
        weapons.update(true, &state, (100, 60), 0, &BARRELS, 0);
        assert_eq!(weapons.shots.len(), ring);

        // The next held tick fires normally.
        weapons.update(true, &state, (100, 60), 0, &BARRELS, 0);
        assert!(weapons.shots.len() > ring);
    }

    #[test]
    fn a_bomb_press_during_the_ring_flight_forces_a_volley() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let state = state(Weapon::Multishot, 2);
        run(&mut weapons, false, &state, 1);

        // Fire released, cooldown cold: the forced volley bypasses both.
        weapons.force_volley();
        weapons.update(false, &state, (100, 60), 0, &BARRELS, 0);
        assert!(!weapons.shots.is_empty());
    }

    #[test]
    fn the_first_missile_locks_the_first_entity() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let state = state(Weapon::Missile, 1);
        run(&mut weapons, false, &state, 1);
        weapons.cooldown = 100;

        // Three enemies live: the counter pre-increments, so the very first
        // missile locks entity 1 and the next one entity 2.
        weapons.update(true, &state, (100, 60), 0, &BARRELS, 3);
        assert_eq!(weapons.shots.last().map(|shot| shot.target), Some(1));

        weapons.cooldown = 100;
        weapons.update(true, &state, (100, 60), 0, &BARRELS, 3);
        assert_eq!(weapons.shots.last().map(|shot| shot.target), Some(2));
    }

    #[test]
    fn the_missile_alternates_its_spawn_row_at_its_slow_rate() {
        let mut weapons = Weapons::new(vec![0; 20], ActiveWeapon::Chaingun, 0x1200, (11, 7));
        let state = state(Weapon::Missile, 1);

        // The rate variable still holds the previous weapon's period until the
        // first missile shot stores 45 (faithful: `cs:[0xcc8]` is set by the
        // spawners), so the first shot lands on the leftover chaingun rate.
        run(&mut weapons, false, &state, 1);
        run(&mut weapons, true, &state, 5);
        assert_eq!(weapons.shots.len(), 1);

        // The second shot follows a full missile period later, on the
        // alternate barrel row.
        run(&mut weapons, true, &state, 45);
        assert_eq!(weapons.shots.len(), 2);
        assert_eq!(weapons.shots[1].y - weapons.shots[0].y, 7 << 4);
    }
}

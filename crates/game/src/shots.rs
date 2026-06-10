//! Player fire: the cooldown state machine, per-weapon spawn patterns, the
//! shot pool, and the chaingun muzzle flash.
//!
//! Reverse-engineered from `LEVEL_1.WAD` (state machine file `0xb68a`,
//! dispatch `0x9a1d`, spawners `0x96f2..`, update loop `0xc35e`); the spawn
//! offsets, velocities, damages and fire rates are identical in all seven
//! WADs. Shots live in window/buffer coordinates like the ship, in 1/16-pixel
//! fixed point, and move by their velocity each tick. They despawn outside
//! x in (-32, 288) and y in (-10, 160), and the pool caps at 95 records (the
//! original's "Too-many-Shots" bound).
//!
//! One fire button, held with auto-repeat: a cooldown counter runs up to the
//! firing weapon's rate (set per shot, level-scaled for the secondaries). The
//! firing weapon re-resolves only while fire is NOT held, freezing it across
//! a burst like the original's resolve gate. Firing restarts the muzzle-flash
//! animation, which only shows while the chaingun is the firing weapon.
//!
//! Not ported yet: the plasma orbs (weapon 3 fires nothing here until their
//! bob tables are reverse-engineered), the missile's homing steer (needs
//! enemies to target), and the spawners' sound triggers.

use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::{ActiveWeapon, GameState, Weapon};

use crate::assets::{FireSprites, OverlaySprite};
use crate::playfield;

/// The shot pool's record cap (the original errors past `0x5f`).
const MAX_SHOTS: usize = 95;

/// Despawn bounds in 1/16-pixel window coordinates (update loop `0xc35e`).
const X_MAX: i32 = 0x1200;
const X_MIN: i32 = -0x200;
const Y_MAX: i32 = 0xa00;
const Y_MIN: i32 = -0xa0;

/// Muzzle-flash animation: the offset steps 8 per tick to `0x30` (6 frames),
/// restarted on every shot.
const FLASH_END: i32 = 0x30;

/// Which sprite a shot draws, resolved against [`FireSprites`] at render.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ShotKind {
    Chaingun,
    /// Charge level 1..=4 picks the sprite.
    Multishot(usize),
    Burning(usize),
    Missile,
}

/// One live shot, in 1/16-pixel window coordinates.
struct Shot {
    kind: ShotKind,
    x: i32,
    y: i32,
    dx: i32,
    dy: i32,
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
    /// Muzzle-flash animation offset (`cs:[0xccb]`), `>= FLASH_END` = idle.
    flash_offset: i32,
    shots: Vec<Shot>,
}

impl Weapons {
    pub fn new() -> Self {
        Self {
            cooldown: 0,
            rate: 6,
            firing: ActiveWeapon::Chaingun,
            missile_toggle: false,
            flash_offset: FLASH_END,
            shots: Vec::new(),
        }
    }

    /// Advance one logic tick: re-resolve the firing weapon (unless frozen by
    /// held fire), run the cooldown, spawn due shots from the ship at `(x, y)`
    /// with roll frame `roll` (for the barrel offsets), move the live shots,
    /// and advance the flash.
    pub fn update(
        &mut self,
        fire_held: bool,
        state: &GameState,
        ship: (i32, i32),
        roll: usize,
        barrels: &[(i32, i32)],
    ) {
        if !fire_held {
            self.firing = state.active_weapon();
        }

        // The cooldown counts up to the rate and holds there; firing resets
        // it to zero (file 0xb68a). Plasma bypasses the counter entirely.
        let due = if self.cooldown < self.rate {
            self.cooldown += 1;
            self.cooldown >= self.rate
        } else {
            true
        };

        let plasma = self.firing == ActiveWeapon::Selected(Weapon::Plasma);

        if fire_held && (due || plasma) {
            self.fire(state, ship, roll, barrels);
            self.cooldown = 0;
        }

        for shot in &mut self.shots {
            shot.x += shot.dx;
            shot.y += shot.dy;
        }

        self.shots
            .retain(|shot| shot.x < X_MAX && shot.x > X_MIN && shot.y < Y_MAX && shot.y > Y_MIN);

        if self.flash_offset < FLASH_END {
            self.flash_offset += 8;
        }
    }

    /// Spawn the firing weapon's shots from ship position `(x, y)` (window
    /// pixels) and set its auto-repeat rate. Restarts the muzzle flash.
    fn fire(&mut self, state: &GameState, (x, y): (i32, i32), roll: usize, barrels: &[(i32, i32)]) {
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
                self.spawn(ShotKind::Burning(level), x, y + 16, 224, 0);
                self.rate = [19, 18, 17, 16][level];
            }
            ActiveWeapon::Selected(Weapon::Plasma) => {
                // The plasma orbs are not ported yet (their trail/bob tables
                // are still unread); the weapon fires nothing for now.
            }
            ActiveWeapon::Selected(Weapon::Missile) => {
                let level = charge_index(state, Weapon::Missile);
                let dy = if self.missile_toggle { 7 } else { 0 };
                self.missile_toggle = !self.missile_toggle;
                self.spawn(ShotKind::Missile, x + 35, y + 11 + dy, 48, 0);
                self.rate = [45, 35, 25, 15][level];
            }
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
        });
    }

    /// Composite the live shots (window coordinates, like the ship).
    pub fn render(&self, sprites: &FireSprites, frame: &mut Framebuffer, camera: i32) {
        for shot in &self.shots {
            let sprite = match shot.kind {
                ShotKind::Chaingun => &sprites.chaingun,
                ShotKind::Multishot(level) => &sprites.multishot[level],
                ShotKind::Burning(level) => &sprites.burning[level],
                ShotKind::Missile => &sprites.missile,
            };

            blit(frame, sprite, shot.x >> 4, (shot.y >> 4) - camera);
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

impl Default for Weapons {
    fn default() -> Self {
        Self::new()
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
        }
    }

    fn run(weapons: &mut Weapons, fire: bool, state: &GameState, ticks: u32) {
        for _ in 0..ticks {
            weapons.update(fire, state, (100, 60), 0, &BARRELS);
        }
    }

    #[test]
    fn the_chaingun_fires_two_barrel_shots_on_its_cooldown() {
        let mut weapons = Weapons::new();
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
        let mut weapons = Weapons::new();
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
        let mut weapons = Weapons::new();
        let low = state(Weapon::Multishot, 1);
        run(&mut weapons, false, &low, 1);
        run(&mut weapons, true, &low, 8);
        assert_eq!(weapons.shots.len(), 3);

        let mut weapons = Weapons::new();
        let max = state(Weapon::Multishot, 4);
        run(&mut weapons, false, &max, 1);
        run(&mut weapons, true, &max, 8);
        assert_eq!(weapons.shots.len(), 5);
        assert!(weapons.shots.iter().any(|shot| shot.dx < 0));
    }

    #[test]
    fn the_firing_weapon_freezes_while_fire_is_held() {
        let mut weapons = Weapons::new();
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
    fn the_missile_alternates_its_spawn_row_at_its_slow_rate() {
        let mut weapons = Weapons::new();
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

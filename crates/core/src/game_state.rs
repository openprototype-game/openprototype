//! In-game player state: the data the level HUD reads, and the rules that drive
//! it.
//!
//! This is the gameplay-facing state (score, lives, smart bombs, the weapon
//! loadout, the respawn shield), distinct from the [`Game`](crate::game::Game)
//! trait, which is the per-frame scene contract. The mutating rules
//! ([`add_score`](GameState::add_score), the orb [`level_up`](GameState::level_up),
//! [`take_hit`](GameState::take_hit), and the rest) are reverse-engineered from
//! the original's weapon and damage code; see `reference/combat.md`.
//!
//! What lives here is the player's own state and its rules. The *consequences*
//! of those rules (the shield sprite, the respawn position, the GET READY
//! sequencing) belong to the scene layer, which reads
//! [`is_invincible`](GameState::is_invincible) and a [`HitOutcome`].

use crate::bounded::BoundedU8;

/// Points between extra lives; one life is granted per boundary crossed.
const EXTRA_LIFE_INTERVAL: u32 = 10_000;

/// Invincibility granted on respawn, in game ticks (the original's `300`, ~3 s).
const RESPAWN_INVINCIBILITY_TICKS: u16 = 300;

/// A secondary weapon's charge level: `0..=4`, one filled bar segment per level.
///
/// A power-up orb raises it by one (the original stores it as pixel fill
/// `0,8,16,24,32`; this is the logical level), clamped to `4`.
pub type WeaponLevel = BoundedU8<4>;

/// Remaining lives: `0..=9` (the original caps the counter at 9).
pub type Lives = BoundedU8<9>;

/// Smart bombs held: `0..=3`.
pub type SmartBombs = BoundedU8<3>;

/// The weapon currently firing and shown in the HUD's right-hand display.
///
/// `Minigun` is the always-available default (the original's effective-weapon
/// value `0`); the four secondaries are the leveled weapons drawn as the BALKEN
/// bars. Variant names are placeholders until the weapon sprites are decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weapon {
    /// The default gun; always available, no charge level.
    #[default]
    Minigun,
    Secondary1,
    Secondary2,
    Secondary3,
    Secondary4,
}

/// One of the four selectable secondary weapons, indexing [`GameState::weapons`].
///
/// This is what the HUD's selector marker points at; the original cycles it
/// `1→2→3→4→1`. Names are placeholders pending the sprite decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Secondary {
    #[default]
    One,
    Two,
    Three,
    Four,
}

impl Secondary {
    /// The four secondaries in selector order, for indexing and iteration.
    pub const ALL: [Secondary; 4] = [
        Secondary::One,
        Secondary::Two,
        Secondary::Three,
        Secondary::Four,
    ];

    /// The array index this secondary occupies in [`GameState::weapons`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// The next secondary in selector order, wrapping `Four → One`.
    pub fn next(self) -> Self {
        match self {
            Secondary::One => Secondary::Two,
            Secondary::Two => Secondary::Three,
            Secondary::Three => Secondary::Four,
            Secondary::Four => Secondary::One,
        }
    }

    /// The [`Weapon`] this secondary fires as.
    pub fn as_weapon(self) -> Weapon {
        match self {
            Secondary::One => Weapon::Secondary1,
            Secondary::Two => Weapon::Secondary2,
            Secondary::Three => Weapon::Secondary3,
            Secondary::Four => Weapon::Secondary4,
        }
    }
}

/// Which collision dealt a hit: ramming a body vs. clipping a projectile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Body contact with an enemy ship: zeroes the firing weapon's charge.
    Collision,
    /// Clipped by an enemy projectile: drains one charge level.
    Bullet,
}

/// What a [`GameState::take_hit`] resolved to, for the scene to react to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitOutcome {
    /// Invincible: the hit was ignored.
    Shielded,
    /// A charged secondary absorbed the hit; no life lost.
    Absorbed,
    /// The hit cost a life; the ship respawns (invincibility armed).
    Died,
    /// The last life was lost.
    GameOver,
}

/// The level HUD's data source and the player-state rules that drive it.
///
/// The starting values for a fresh game are not encoded here — the original's
/// new-game init is not traced yet, so constructing a `GameState` is the
/// caller's responsibility for now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameState {
    /// Points, shown as the six-digit LCD readout.
    pub score: u32,

    /// Remaining lives, shown as a single digit.
    pub lives: Lives,

    /// Smart bombs held, shown by the smart-bomb indicator.
    pub smart_bombs: SmartBombs,

    /// The four secondary charge bars, indexed by [`Secondary::index`].
    pub weapons: [WeaponLevel; 4],

    /// Which secondary the selector marker highlights.
    pub selected: Secondary,

    /// Remaining post-respawn invincibility, in game ticks (`0` = vulnerable).
    pub invincible_ticks: u16,
}

impl GameState {
    /// Returns the charge level of one secondary.
    pub fn level(&self, secondary: Secondary) -> WeaponLevel {
        self.weapons[secondary.index()]
    }

    /// The weapon that actually fires: the selected secondary while it holds
    /// charge, otherwise the always-available minigun.
    ///
    /// The original derives this each frame (and freezes it while fire is held);
    /// here it is a pure getter, leaving the freeze to the input layer.
    pub fn firing_weapon(&self) -> Weapon {
        if self.level(self.selected).get() >= 1 {
            self.selected.as_weapon()
        } else {
            Weapon::Minigun
        }
    }

    /// Adds `points`, granting an extra life for each 10,000-point boundary
    /// crossed (lives cap at 9).
    pub fn add_score(&mut self, points: u32) {
        let milestones_before = self.score / EXTRA_LIFE_INTERVAL;
        self.score = self.score.saturating_add(points);
        let earned = self.score / EXTRA_LIFE_INTERVAL - milestones_before;

        if earned > 0 {
            let earned = u8::try_from(earned).unwrap_or(u8::MAX);
            self.lives = self.lives.saturating_add(earned);
        }
    }

    /// Raises the selected secondary's charge by one (an orb pickup), capped at
    /// [`WeaponLevel::MAX`].
    pub fn level_up(&mut self) {
        let slot = self.selected.index();
        self.weapons[slot] = self.weapons[slot].saturating_add(1);
    }

    /// Cycles the selector to the next secondary, wrapping `Four → One`.
    pub fn cycle_weapon(&mut self) {
        self.selected = self.selected.next();
    }

    /// Spends a smart bomb if any are held; returns whether one fired.
    pub fn use_smart_bomb(&mut self) -> bool {
        if self.smart_bombs.get() == 0 {
            return false;
        }

        self.smart_bombs = self.smart_bombs.saturating_sub(1);
        true
    }

    /// Counts the invincibility timer down by one tick.
    pub fn tick(&mut self) {
        self.invincible_ticks = self.invincible_ticks.saturating_sub(1);
    }

    /// Whether the post-respawn shield is still up (no damage lands).
    pub fn is_invincible(&self) -> bool {
        self.invincible_ticks > 0
    }

    /// Applies a hit. While invincible nothing happens; otherwise a charged
    /// secondary absorbs it (zeroed by a [`Collision`](Severity::Collision),
    /// drained one level by a [`Bullet`](Severity::Bullet)) and the ship
    /// survives, while a hit on the bare minigun costs a life. See
    /// `reference/combat.md`.
    pub fn take_hit(&mut self, severity: Severity) -> HitOutcome {
        if self.is_invincible() {
            return HitOutcome::Shielded;
        }

        if self.firing_weapon() == Weapon::Minigun {
            return self.lose_life();
        }

        let slot = self.selected.index();
        self.weapons[slot] = match severity {
            Severity::Collision => WeaponLevel::new(0),
            Severity::Bullet => self.weapons[slot].saturating_sub(1),
        };

        HitOutcome::Absorbed
    }

    /// Drops a life. Arms respawn invincibility if any lives remain, otherwise
    /// reports game over.
    pub fn lose_life(&mut self) -> HitOutcome {
        self.lives = self.lives.saturating_sub(1);

        if self.lives.get() == 0 {
            HitOutcome::GameOver
        } else {
            self.invincible_ticks = RESPAWN_INVINCIBILITY_TICKS;
            HitOutcome::Died
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> GameState {
        GameState {
            score: 0,
            lives: Lives::new(3),
            smart_bombs: SmartBombs::new(0),
            weapons: [WeaponLevel::new(0); 4],
            selected: Secondary::One,
            invincible_ticks: 0,
        }
    }

    fn with_selected_level(selected: Secondary, level: u8) -> GameState {
        let mut state = fresh();
        state.selected = selected;
        state.weapons[selected.index()] = WeaponLevel::new(level);
        state
    }

    #[test]
    fn firing_weapon_is_minigun_when_the_selected_slot_is_empty() {
        assert_eq!(fresh().firing_weapon(), Weapon::Minigun);
    }

    #[test]
    fn firing_weapon_is_the_selected_secondary_once_charged() {
        let state = with_selected_level(Secondary::Three, 1);
        assert_eq!(state.firing_weapon(), Weapon::Secondary3);
    }

    #[test]
    fn firing_weapon_ignores_charge_in_an_unselected_slot() {
        let mut state = fresh();
        state.weapons[Secondary::Two.index()] = WeaponLevel::new(4);
        assert_eq!(state.firing_weapon(), Weapon::Minigun);
    }

    #[test]
    fn add_score_grants_a_life_on_each_ten_thousand_boundary() {
        let mut state = fresh();
        state.add_score(10_000);
        assert_eq!(state.lives.get(), 4);

        let mut multi = fresh();
        multi.add_score(25_000);
        assert_eq!(multi.lives.get(), 5);
    }

    #[test]
    fn add_score_grants_no_life_below_a_boundary() {
        let mut state = fresh();
        state.add_score(9_000);
        assert_eq!(state.score, 9_000);
        assert_eq!(state.lives.get(), 3);

        state.add_score(1_000);
        assert_eq!(state.lives.get(), 4);
    }

    #[test]
    fn add_score_caps_lives_at_nine() {
        let mut state = fresh();
        state.lives = Lives::new(9);
        state.add_score(10_000);
        assert_eq!(state.lives.get(), 9);
    }

    #[test]
    fn level_up_charges_the_selected_slot_and_caps_at_four() {
        let mut state = fresh();
        state.level_up();
        assert_eq!(state.level(Secondary::One).get(), 1);

        for _ in 0..10 {
            state.level_up();
        }

        assert_eq!(state.level(Secondary::One).get(), 4);
        assert_eq!(state.level(Secondary::Two).get(), 0);
    }

    #[test]
    fn cycle_weapon_advances_and_wraps() {
        let mut state = fresh();
        assert_eq!(state.selected, Secondary::One);
        state.cycle_weapon();
        assert_eq!(state.selected, Secondary::Two);
        state.cycle_weapon();
        state.cycle_weapon();
        assert_eq!(state.selected, Secondary::Four);
        state.cycle_weapon();
        assert_eq!(state.selected, Secondary::One);
    }

    #[test]
    fn use_smart_bomb_spends_one_when_held() {
        let mut state = fresh();
        state.smart_bombs = SmartBombs::new(2);
        assert!(state.use_smart_bomb());
        assert_eq!(state.smart_bombs.get(), 1);
    }

    #[test]
    fn use_smart_bomb_does_nothing_when_empty() {
        let mut state = fresh();
        assert!(!state.use_smart_bomb());
        assert_eq!(state.smart_bombs.get(), 0);
    }

    #[test]
    fn tick_counts_invincibility_down_and_saturates() {
        let mut state = fresh();
        state.invincible_ticks = 2;
        assert!(state.is_invincible());
        state.tick();
        assert!(state.is_invincible());
        state.tick();
        assert!(!state.is_invincible());
        state.tick();
        assert_eq!(state.invincible_ticks, 0);
    }

    #[test]
    fn take_hit_is_ignored_while_invincible() {
        let mut state = with_selected_level(Secondary::One, 3);
        state.invincible_ticks = 10;
        assert_eq!(state.take_hit(Severity::Collision), HitOutcome::Shielded);
        assert_eq!(state.level(Secondary::One).get(), 3);
        assert_eq!(state.lives.get(), 3);
        assert_eq!(state.invincible_ticks, 10);
    }

    #[test]
    fn collision_zeroes_a_charged_secondary_without_costing_a_life() {
        let mut state = with_selected_level(Secondary::One, 3);
        assert_eq!(state.take_hit(Severity::Collision), HitOutcome::Absorbed);
        assert_eq!(state.level(Secondary::One).get(), 0);
        assert_eq!(state.firing_weapon(), Weapon::Minigun);
        assert_eq!(state.lives.get(), 3);
    }

    #[test]
    fn bullet_drains_one_level_of_a_charged_secondary() {
        let mut state = with_selected_level(Secondary::One, 3);
        assert_eq!(state.take_hit(Severity::Bullet), HitOutcome::Absorbed);
        assert_eq!(state.level(Secondary::One).get(), 2);
    }

    #[test]
    fn bullet_emptying_the_last_level_reverts_to_the_minigun() {
        let mut state = with_selected_level(Secondary::One, 1);
        assert_eq!(state.take_hit(Severity::Bullet), HitOutcome::Absorbed);
        assert_eq!(state.level(Secondary::One).get(), 0);
        assert_eq!(state.firing_weapon(), Weapon::Minigun);
    }

    #[test]
    fn a_hit_on_the_minigun_costs_a_life_and_arms_the_shield() {
        let mut state = fresh();
        assert_eq!(state.take_hit(Severity::Collision), HitOutcome::Died);
        assert_eq!(state.lives.get(), 2);
        assert_eq!(state.invincible_ticks, 300);
    }

    #[test]
    fn the_last_life_lost_is_game_over_with_no_shield() {
        let mut state = fresh();
        state.lives = Lives::new(1);
        assert_eq!(state.take_hit(Severity::Bullet), HitOutcome::GameOver);
        assert_eq!(state.lives.get(), 0);
        assert_eq!(state.invincible_ticks, 0);
    }

    #[test]
    fn lose_life_arms_the_shield_while_lives_remain() {
        let mut state = fresh();
        assert_eq!(state.lose_life(), HitOutcome::Died);
        assert_eq!(state.lives.get(), 2);
        assert_eq!(state.invincible_ticks, 300);

        state.invincible_ticks = 0;
        state.lives = Lives::new(1);
        assert_eq!(state.lose_life(), HitOutcome::GameOver);
        assert_eq!(state.invincible_ticks, 0);
    }

    #[test]
    fn two_hits_in_a_row_zero_the_bar_then_take_the_life() {
        let mut state = with_selected_level(Secondary::Two, 4);
        assert_eq!(state.take_hit(Severity::Collision), HitOutcome::Absorbed);
        assert_eq!(state.firing_weapon(), Weapon::Minigun);
        assert_eq!(state.lives.get(), 3);

        assert_eq!(state.take_hit(Severity::Collision), HitOutcome::Died);
        assert_eq!(state.lives.get(), 2);
        assert_eq!(state.invincible_ticks, 300);
    }
}

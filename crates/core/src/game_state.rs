//! In-game player state: what the level HUD reads, and the rules driving it.
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

/// A weapon's charge level: `0..=4`, one filled bar segment per level.
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
/// One of the four real weapons the player picks up, charges, and selects (drawn
/// as the BALKEN charge bars). The always-available chaingun fallback is not one
/// of these (see [`ActiveWeapon`]). Names are from the in-game weapon pods and
/// fire sounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Weapon {
    #[default]
    Multishot,
    Burning,
    Plasma,
    Missile,
}

impl Weapon {
    /// The four weapons in selector order, for iteration and the HUD bar layout.
    pub const ALL: [Weapon; 4] = [
        Weapon::Multishot,
        Weapon::Burning,
        Weapon::Plasma,
        Weapon::Missile,
    ];

    /// The next weapon in selector order, wrapping `Missile → Multishot`.
    pub fn next(self) -> Self {
        match self {
            Weapon::Multishot => Weapon::Burning,
            Weapon::Burning => Weapon::Plasma,
            Weapon::Plasma => Weapon::Missile,
            Weapon::Missile => Weapon::Multishot,
        }
    }
}

/// The weapon currently firing.
///
/// Either the always-available chaingun fallback, or the selected real weapon
/// once it holds charge. The original derives this each frame (value `0` =
/// chaingun) and freezes it while fire is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveWeapon {
    /// The default gun (no charge), fired when the selected weapon is empty.
    #[default]
    Chaingun,
    /// The selected real weapon, firing.
    Selected(Weapon),
}

impl From<Weapon> for ActiveWeapon {
    fn from(weapon: Weapon) -> Self {
        ActiveWeapon::Selected(weapon)
    }
}

/// One `T` per real [`Weapon`], addressed by weapon, not a positional index.
///
/// A total mapping: every weapon always holds a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PerWeapon<T> {
    pub multishot: T,
    pub burning: T,
    pub plasma: T,
    pub missile: T,
}

impl<T> PerWeapon<T> {
    /// The value held for one weapon.
    pub fn get(&self, weapon: Weapon) -> &T {
        match weapon {
            Weapon::Multishot => &self.multishot,
            Weapon::Burning => &self.burning,
            Weapon::Plasma => &self.plasma,
            Weapon::Missile => &self.missile,
        }
    }

    /// A mutable reference to the value held for one weapon.
    pub fn get_mut(&mut self, weapon: Weapon) -> &mut T {
        match weapon {
            Weapon::Multishot => &mut self.multishot,
            Weapon::Burning => &mut self.burning,
            Weapon::Plasma => &mut self.plasma,
            Weapon::Missile => &mut self.missile,
        }
    }

    /// Maps each value through `f`, preserving the per-weapon shape.
    pub fn map<U>(self, mut f: impl FnMut(T) -> U) -> PerWeapon<U> {
        PerWeapon {
            multishot: f(self.multishot),
            burning: f(self.burning),
            plasma: f(self.plasma),
            missile: f(self.missile),
        }
    }
}

impl<T: Copy> PerWeapon<T> {
    /// The same value for every weapon.
    pub fn splat(value: T) -> Self {
        Self {
            multishot: value,
            burning: value,
            plasma: value,
            missile: value,
        }
    }
}

/// Which collision dealt a hit: ramming a body vs. clipping a projectile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Body contact with an enemy ship: zeroes the selected weapon's charge.
    Collision,
    /// Clipped by an enemy projectile: drains one charge level.
    Bullet,
}

/// What a [`GameState::take_hit`] resolved to, for the scene to react to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitOutcome {
    /// Invincible: the hit was ignored.
    Shielded,
    /// A charged weapon absorbed the hit; no life lost.
    Absorbed,
    /// A fatal hit: the death sequence starts. The life itself is deducted
    /// when the sequence ends ([`GameState::lose_life`]), like the
    /// original's respawn-time decrement.
    Died,
    /// The last life was lost ([`GameState::lose_life`] only).
    GameOver,
}

/// The between-levels carry: the original's `f:message` payload.
///
/// The payload is `{score, lives, bombs, weapon levels}`. START.EXE seeds it
/// for a new game, every level writes it back at exit, and the next level
/// reads it at entry through [`GameState::enter_level`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handoff {
    pub score: u32,
    /// The raw carried counter; the entry decrement is not applied yet, so a
    /// fresh game carries 4 and the first level displays 3.
    pub lives: Lives,
    pub smart_bombs: SmartBombs,
    pub weapons: PerWeapon<WeaponLevel>,
}

impl Handoff {
    /// START.EXE's new-game payload (its writer at file `0x3c9e`).
    ///
    /// Score 0, lives 4, one smart bomb, bare weapons.
    pub fn new_game() -> Handoff {
        Handoff {
            score: 0,
            lives: Lives::new(4),
            smart_bombs: SmartBombs::new(1),
            weapons: PerWeapon::splat(WeaponLevel::new(0)),
        }
    }
}

/// The level HUD's data source and the player-state rules that drive it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameState {
    /// Points, shown as the six-digit LCD readout.
    pub score: u32,

    /// Remaining lives, shown as a single digit.
    pub lives: Lives,

    /// Smart bombs held, shown by the smart-bomb indicator.
    pub smart_bombs: SmartBombs,

    /// The four weapons' charge levels.
    pub weapons: PerWeapon<WeaponLevel>,

    /// Which weapon the selector marker highlights.
    pub selected: Weapon,

    /// Remaining post-respawn invincibility, in game ticks (`0` = vulnerable).
    pub invincible_ticks: u16,

    /// Race mode's contact grace: ticks left during which obstacle contact
    /// is ignored (`cs:0x284e`; stays 0 in the shooters).
    pub contact_grace_ticks: u16,
}

impl GameState {
    /// Builds the level-entry state from the carried payload.
    ///
    /// The original's level init runs two unfrozen warm-up frames in which
    /// the per-tick score updater grants the 10,000-point extra life (the
    /// threshold is per-level state that resets to zero), and only then does
    /// the spawn handler take its entry decrement — so entering a level nets
    /// `lives + (score >= 10,000) - 1`. A carry of one life with under
    /// 10,000 points nets zero: the original exits straight to game over
    /// before the level even shows (verified in DOSBox), surfaced here as a
    /// zero-lives state for the scene to route. The warm-up refund's jingle
    /// is not reproduced.
    ///
    /// Invincibility is left unarmed: the spawn shield is per-level data the
    /// scene applies on top.
    pub fn enter_level(handoff: Handoff) -> GameState {
        let refund = u8::from(handoff.score >= EXTRA_LIFE_INTERVAL);

        GameState {
            score: handoff.score,
            lives: handoff.lives.saturating_add(refund).saturating_sub(1),
            smart_bombs: handoff.smart_bombs,
            weapons: handoff.weapons,
            selected: Weapon::Multishot,
            invincible_ticks: 0,
            contact_grace_ticks: 0,
        }
    }

    /// The level-exit writeback: the carry for the next level.
    ///
    /// The lives counter is stored raw (the original's writer at file `0xbb8e`
    /// stores it as-is; the next entry applies its own decrement).
    pub fn handoff(&self) -> Handoff {
        Handoff {
            score: self.score,
            lives: self.lives,
            smart_bombs: self.smart_bombs,
            weapons: self.weapons,
        }
    }

    /// Returns the charge level of one weapon.
    pub fn level(&self, weapon: Weapon) -> WeaponLevel {
        *self.weapons.get(weapon)
    }

    /// The weapon that actually fires.
    ///
    /// The selected weapon while it holds charge, otherwise the always-available
    /// chaingun. The original derives this each frame (and freezes it while fire
    /// is held); here it is a pure getter, leaving the freeze to the input layer.
    pub fn active_weapon(&self) -> ActiveWeapon {
        if self.level(self.selected).get() >= 1 {
            self.selected.into()
        } else {
            ActiveWeapon::Chaingun
        }
    }

    /// Adds `points`, granting an extra life per 10,000-point boundary.
    ///
    /// At most one life is granted per call (lives cap at 9). Returns whether a
    /// life was granted, for the scene's jingle trigger.
    ///
    /// The original's per-tick updater (`div 0x2710`, all seven WADs) grants one
    /// life per check and snaps its milestone to the full quotient, so crossing
    /// two boundaries in one tick (a smart bomb reaping a dense wave) still
    /// pays one life and never pays the skipped boundary later. The snap is
    /// implicit here: the milestone derives from the score itself.
    pub fn add_score(&mut self, points: u32) -> bool {
        let milestones_before = self.score / EXTRA_LIFE_INTERVAL;
        self.score = self.score.saturating_add(points);
        let crossed = self.score / EXTRA_LIFE_INTERVAL > milestones_before;

        if crossed {
            self.lives = self.lives.saturating_add(1);
        }

        crossed
    }

    /// Raises the selected weapon's charge by one (an orb pickup).
    ///
    /// Capped at [`WeaponLevel::MAX`].
    pub fn level_up(&mut self) {
        let level = self.weapons.get_mut(self.selected);
        *level = level.saturating_add(1);
    }

    /// Cycles the selector to the next weapon, wrapping `Missile → Multishot`.
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

    /// Counts the invincibility and contact-grace timers down by one tick.
    pub fn tick(&mut self) {
        self.invincible_ticks = self.invincible_ticks.saturating_sub(1);
        self.contact_grace_ticks = self.contact_grace_ticks.saturating_sub(1);
    }

    /// Whether the post-respawn shield is still up (no damage lands).
    pub fn is_invincible(&self) -> bool {
        self.invincible_ticks > 0
    }

    /// Applies a hit.
    ///
    /// While invincible nothing happens; otherwise the `firing` weapon absorbs
    /// it (zeroed by a [`Collision`](Severity::Collision), drained one level by a
    /// [`Bullet`](Severity::Bullet)) and the ship survives, while a hit on the
    /// bare chaingun is fatal. See `reference/combat.md`.
    ///
    /// `firing` is the resolved firing weapon (`cs:0xcb5`), owned by the fire
    /// system because it freezes across a held burst: after a mid-burst weapon
    /// cycle it differs from `selected`, and the original's hit consequence (file
    /// `0xc4b7`) and ram zero-out (file `0xdcf1`) both index off it, not the
    /// selector.
    ///
    /// A fatal hit does not touch the lives counter: the caller plays the death
    /// sequence and calls [`Self::lose_life`] when it ends, matching the original
    /// (the dying flag is set at the hit, the decrement and the game-over check
    /// live in the respawn handler).
    pub fn take_hit(&mut self, severity: Severity, firing: ActiveWeapon) -> HitOutcome {
        if self.is_invincible() {
            return HitOutcome::Shielded;
        }

        let ActiveWeapon::Selected(weapon) = firing else {
            return HitOutcome::Died;
        };

        let level = self.weapons.get_mut(weapon);
        *level = match severity {
            Severity::Collision => WeaponLevel::new(0),
            Severity::Bullet => level.saturating_sub(1),
        };

        HitOutcome::Absorbed
    }

    /// Drops a life.
    ///
    /// Arms `respawn_invincibility` ticks if any lives remain, otherwise reports
    /// game over. The duration is per level (L3's respawn handler writes 180
    /// ticks, the others 300).
    pub fn lose_life(&mut self, respawn_invincibility: u16) -> HitOutcome {
        self.lives = self.lives.saturating_sub(1);

        if self.lives.get() == 0 {
            HitOutcome::GameOver
        } else {
            self.invincible_ticks = respawn_invincibility;
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
            weapons: PerWeapon::default(),
            selected: Weapon::Multishot,
            invincible_ticks: 0,
            contact_grace_ticks: 0,
        }
    }

    fn with_selected_level(selected: Weapon, level: u8) -> GameState {
        let mut state = fresh();
        state.selected = selected;
        *state.weapons.get_mut(selected) = WeaponLevel::new(level);
        state
    }

    #[test]
    fn entering_a_new_game_nets_three_lives_and_one_bomb() {
        let state = GameState::enter_level(Handoff::new_game());

        assert_eq!(state.score, 0);
        assert_eq!(state.lives.get(), 3);
        assert_eq!(state.smart_bombs.get(), 1);
        assert_eq!(state.active_weapon(), ActiveWeapon::Chaingun);
        assert_eq!(state.invincible_ticks, 0);
    }

    #[test]
    fn entering_with_a_ten_thousand_score_refunds_the_entry_decrement() {
        let mut carry = Handoff::new_game();
        carry.score = 25_000;
        carry.lives = Lives::new(3);

        assert_eq!(GameState::enter_level(carry).lives.get(), 3);
    }

    #[test]
    fn entering_under_ten_thousand_pays_the_entry_decrement() {
        let mut carry = Handoff::new_game();
        carry.score = 5_000;
        carry.lives = Lives::new(3);

        assert_eq!(GameState::enter_level(carry).lives.get(), 2);
    }

    #[test]
    fn entering_on_the_last_life_without_the_refund_is_a_lost_game() {
        let mut carry = Handoff::new_game();
        carry.score = 5_000;
        carry.lives = Lives::new(1);

        assert_eq!(GameState::enter_level(carry).lives.get(), 0);
    }

    #[test]
    fn the_handoff_writes_the_lives_counter_raw() {
        let mut state = fresh();
        state.score = 12_345;
        let carry = state.handoff();

        assert_eq!(carry.score, 12_345);
        assert_eq!(carry.lives.get(), 3);
    }

    #[test]
    fn active_weapon_is_chaingun_when_the_selected_slot_is_empty() {
        assert_eq!(fresh().active_weapon(), ActiveWeapon::Chaingun);
    }

    #[test]
    fn active_weapon_is_the_selected_weapon_once_charged() {
        let state = with_selected_level(Weapon::Plasma, 1);
        assert_eq!(
            state.active_weapon(),
            ActiveWeapon::Selected(Weapon::Plasma)
        );
    }

    #[test]
    fn active_weapon_ignores_charge_in_an_unselected_slot() {
        let mut state = fresh();
        *state.weapons.get_mut(Weapon::Burning) = WeaponLevel::new(4);
        assert_eq!(state.active_weapon(), ActiveWeapon::Chaingun);
    }

    #[test]
    fn add_score_grants_at_most_one_life_per_check() {
        let mut state = fresh();
        assert!(state.add_score(10_000));
        assert_eq!(state.lives.get(), 4);
        assert!(!state.add_score(1));

        // Two boundaries in one add still pay a single life, and the
        // skipped boundary is never paid later (the milestone snaps).
        let mut multi = fresh();
        multi.add_score(25_000);
        assert_eq!(multi.lives.get(), 4);
        multi.add_score(1_000);
        assert_eq!(multi.lives.get(), 4);
        multi.add_score(5_000);
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
        assert_eq!(state.level(Weapon::Multishot).get(), 1);

        for _ in 0..10 {
            state.level_up();
        }

        assert_eq!(state.level(Weapon::Multishot).get(), 4);
        assert_eq!(state.level(Weapon::Burning).get(), 0);
    }

    #[test]
    fn cycle_weapon_advances_and_wraps() {
        let mut state = fresh();
        assert_eq!(state.selected, Weapon::Multishot);
        state.cycle_weapon();
        assert_eq!(state.selected, Weapon::Burning);
        state.cycle_weapon();
        state.cycle_weapon();
        assert_eq!(state.selected, Weapon::Missile);
        state.cycle_weapon();
        assert_eq!(state.selected, Weapon::Multishot);
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
        let mut state = with_selected_level(Weapon::Multishot, 3);
        state.invincible_ticks = 10;
        assert_eq!(
            state.take_hit(Severity::Collision, Weapon::Multishot.into()),
            HitOutcome::Shielded
        );
        assert_eq!(state.level(Weapon::Multishot).get(), 3);
        assert_eq!(state.lives.get(), 3);
        assert_eq!(state.invincible_ticks, 10);
    }

    #[test]
    fn collision_zeroes_a_charged_weapon_without_costing_a_life() {
        let mut state = with_selected_level(Weapon::Multishot, 3);
        assert_eq!(
            state.take_hit(Severity::Collision, Weapon::Multishot.into()),
            HitOutcome::Absorbed
        );
        assert_eq!(state.level(Weapon::Multishot).get(), 0);
        assert_eq!(state.active_weapon(), ActiveWeapon::Chaingun);
        assert_eq!(state.lives.get(), 3);
    }

    #[test]
    fn bullet_drains_one_level_of_a_charged_weapon() {
        let mut state = with_selected_level(Weapon::Multishot, 3);
        assert_eq!(
            state.take_hit(Severity::Bullet, Weapon::Multishot.into()),
            HitOutcome::Absorbed
        );
        assert_eq!(state.level(Weapon::Multishot).get(), 2);
    }

    #[test]
    fn bullet_emptying_the_last_level_reverts_to_the_chaingun() {
        let mut state = with_selected_level(Weapon::Multishot, 1);
        assert_eq!(
            state.take_hit(Severity::Bullet, Weapon::Multishot.into()),
            HitOutcome::Absorbed
        );
        assert_eq!(state.level(Weapon::Multishot).get(), 0);
        assert_eq!(state.active_weapon(), ActiveWeapon::Chaingun);
    }

    #[test]
    fn a_hit_on_the_chaingun_is_fatal_but_defers_the_life_loss() {
        let mut state = fresh();
        assert_eq!(
            state.take_hit(Severity::Collision, ActiveWeapon::Chaingun),
            HitOutcome::Died
        );
        assert_eq!(state.lives.get(), 3);
        assert_eq!(state.invincible_ticks, 0);
    }

    #[test]
    fn a_hit_drains_the_frozen_firing_weapon_not_the_selection() {
        // Cycling weapons mid-burst moves the selector while the firing
        // weapon stays frozen; the hit lands on the frozen one.
        let mut state = with_selected_level(Weapon::Burning, 2);
        *state.weapons.get_mut(Weapon::Missile) = WeaponLevel::new(2);
        state.selected = Weapon::Missile;

        assert_eq!(
            state.take_hit(Severity::Bullet, Weapon::Burning.into()),
            HitOutcome::Absorbed
        );
        assert_eq!(state.level(Weapon::Burning).get(), 1);
        assert_eq!(state.level(Weapon::Missile).get(), 2);
    }

    #[test]
    fn lose_life_arms_the_shield_while_lives_remain() {
        let mut state = fresh();
        assert_eq!(state.lose_life(300), HitOutcome::Died);
        assert_eq!(state.lives.get(), 2);
        assert_eq!(state.invincible_ticks, 300);

        state.invincible_ticks = 0;
        state.lives = Lives::new(1);
        assert_eq!(state.lose_life(300), HitOutcome::GameOver);
        assert_eq!(state.invincible_ticks, 0);
    }

    #[test]
    fn two_hits_in_a_row_zero_the_bar_then_turn_fatal() {
        let mut state = with_selected_level(Weapon::Burning, 4);
        assert_eq!(
            state.take_hit(Severity::Collision, Weapon::Burning.into()),
            HitOutcome::Absorbed
        );
        assert_eq!(state.active_weapon(), ActiveWeapon::Chaingun);
        assert_eq!(state.lives.get(), 3);

        // The fire system reverted the firing weapon after the zero-out.
        assert_eq!(
            state.take_hit(Severity::Collision, ActiveWeapon::Chaingun),
            HitOutcome::Died
        );
        assert_eq!(state.lives.get(), 3);
    }
}

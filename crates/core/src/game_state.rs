//! In-game player state: the data the level HUD reads to draw itself.
//!
//! This is the gameplay-facing state (score, lives, smart bombs, the weapon
//! loadout), distinct from the [`Game`](crate::game::Game) trait, which is the
//! per-frame scene contract. For now it is **fields only**: the HUD renderer
//! reads these to draw. The rules that mutate them (scoring and the extra-life
//! threshold, weapon switching/firing, the orb level-up, the death reset) are
//! deliberately deferred until the original's weapon state machine is traced as
//! a whole, rather than guessed piecemeal. See `re/hud-and-gamestate.md`.

/// A secondary weapon's charge level: `0..=4`, one filled bar segment per level.
///
/// A power-up orb raises the level by one (the original stores it as pixel fill
/// `0,8,16,24,32`; this is the logical level). [`new`](WeaponLevel::new) clamps
/// to [`MAX`](WeaponLevel::MAX). No mutators yet — the level-up / death-reset
/// rules land with the weapon state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WeaponLevel(u8);

impl WeaponLevel {
    /// The highest a weapon level reaches (4 orbs fill the bar).
    pub const MAX: u8 = 4;

    /// Builds a level, clamping to `0..=`[`MAX`](WeaponLevel::MAX).
    pub fn new(level: u8) -> Self {
        Self(level.min(Self::MAX))
    }

    /// Returns the level as a plain `0..=4`.
    pub fn get(self) -> u8 {
        self.0
    }
}

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
}

/// The level HUD's data source: score, lives, smart bombs, and weapon loadout.
///
/// Read by the HUD renderer; not yet mutated by gameplay (see the module docs).
/// The starting values for a fresh game are not encoded here — the original's
/// new-game init is not traced yet, so constructing a `GameState` is the
/// caller's responsibility for now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameState {
    /// Points, shown as the six-digit LCD readout.
    pub score: u32,

    /// Remaining lives, shown as a single digit (the original caps this at 9).
    pub lives: u8,

    /// Smart bombs held, shown by the smart-bomb indicator.
    pub smart_bombs: u8,

    /// The four secondary charge bars, indexed by [`Secondary::index`].
    pub weapons: [WeaponLevel; 4],

    /// The weapon firing and shown in the right-hand display (the "blob").
    pub current: Weapon,

    /// Which secondary the selector marker highlights.
    pub selected: Secondary,
}

impl GameState {
    /// Returns the charge level of one secondary.
    pub fn level(&self, secondary: Secondary) -> WeaponLevel {
        self.weapons[secondary.index()]
    }
}

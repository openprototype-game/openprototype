//! LEVEL_5 (TECHNO): the layout generator and the enemy AI, together.
//!
//! [`layout`] builds the level's enemy/pickup spawn placement from the
//! PRNG-driven script; [`ai`] runs the per-type behaviour for those spawns.
//! Both speak the same sprite vocabulary.

pub mod ai;
pub mod layout;

pub use layout::script;

// Spawn-sprite identities (cs-offsets into the descriptor table). Enemies are
// named for the rendered look + the behaviour the spawn rows give them (TECHNO,
// so a mechanical fleet); pickups for the effect combat.rs grants, read as
// `[orb, smart_bomb, invincibility, extra_life]`. Same pickup art as L1.

pub const WEAPON_UPGRADE: u16 = 0x3764;
pub const SMART_BOMB: u16 = 0x3688;
pub const INVINCIBILITY: u16 = 0x36ee;
pub const EXTRA_LIFE: u16 = 0x382a;

pub const RAIDER: u16 = 0x3a2c;
pub const GUNSHIP: u16 = 0x3ac2;
pub const TANK: u16 = 0x3b70;
pub const DRONE_L: u16 = 0x3c4e;
pub const DRONE_R: u16 = 0x3c84;
pub const FIGHTER: u16 = 0x3cf0;
pub const DESTROYER: u16 = 0x3d46;
pub const BOSS: u16 = 0x426e;

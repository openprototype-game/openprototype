//! LEVEL_1 (CANYON): the layout generator and the enemy AI, together.
//!
//! [`layout`] builds the level's enemy/pickup spawn placement from the
//! PRNG-driven script; [`ai`] runs the per-type behaviour for those spawns.
//! Both speak the same sprite vocabulary.

pub mod ai;
pub mod layout;

pub use layout::script;

// Spawn-sprite identities (cs-offsets into the descriptor table). Enemies are
// named for the AI behaviour that drives them (see [`ai`]); pickups for the
// effect they grant on collision (combat.rs reads the four as
// `[orb, smart_bomb, invincibility, extra_life]`). Verified against the art.

pub const ASTEROID: u16 = 0x3308;
pub const CANNON: u16 = 0x338e;
pub const SNIPER: u16 = 0x33f4;
pub const KAMIKAZE: u16 = 0x38b0;
pub const ORBITER: u16 = 0x392e;
pub const STRAFER: u16 = 0x39a4;
pub const INTERCEPTOR: u16 = 0x3a92;
pub const BOSS: u16 = 0x3f8e;

pub const WEAPON_UPGRADE: u16 = 0x36ea;
pub const SMART_BOMB: u16 = 0x3750;
pub const INVINCIBILITY: u16 = 0x37b6;
pub const EXTRA_LIFE: u16 = 0x382c;

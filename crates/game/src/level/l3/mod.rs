//! LEVEL_3 (WALD): the layout generator and the enemy AI, together.
//!
//! [`layout`] builds the level's enemy/pickup spawn placement from the
//! PRNG-driven script and overwrite post-pass; [`ai`] runs the per-type
//! behaviour for those spawns. Both speak the same sprite vocabulary.

pub mod ai;
pub mod layout;

pub use layout::{post_pass, script};

// Spawn-sprite identities (cs-offsets into the descriptor table). Enemies are
// named for their rendered look + the behaviour the spawn rows give them;
// pickups for the effect combat.rs grants, read as
// `[orb, smart_bomb, invincibility, extra_life]`. The _L/_R pairs are the same
// creature facing left/right.

pub const WEAPON_UPGRADE: u16 = 0x51e8;
pub const SMART_BOMB: u16 = 0x510c;
pub const INVINCIBILITY: u16 = 0x5172;
pub const EXTRA_LIFE: u16 = 0x52ae;

pub const MOSQUITO: u16 = 0x54b0;
pub const RED_BEETLE: u16 = 0x54d6;
pub const BLUE_BEETLE: u16 = 0x57e2;
pub const PTERODACTYL: u16 = 0x56b6;
pub const WASP_L: u16 = 0x5818;
pub const WASP_R: u16 = 0x583e;
pub const WASPLING_L: u16 = 0x5864;
pub const WASPLING_R: u16 = 0x588a;
pub const BAT: u16 = 0x5928;
pub const BAT_HANGING: u16 = 0x58b0;
pub const LURKER: u16 = 0x5ac4;
pub const BOSS: u16 = 0x5c20;

//! LEVEL_7 (CITY): the layout generator and the enemy AI, together.
//!
//! [`layout`] builds the level's enemy/pickup spawn placement from the
//! PRNG-driven script and insert post-pass; [`ai`] runs the per-type
//! behaviour for those spawns. Both speak the same sprite vocabulary.

pub mod ai;
pub mod layout;

pub use layout::{post_pass, script};

// Spawn-sprite identities (cs-offsets into the descriptor table). Enemies are
// named for the rendered look + the behaviour the spawn rows give them (CITY);
// pickups for the effect combat.rs grants, read as
// `[orb, smart_bomb, invincibility, extra_life]`. Same pickup art as L1. The
// boss is one composite creature: a controller plus four parts.

pub const WEAPON_UPGRADE: u16 = 0x4291;
pub const SMART_BOMB: u16 = 0x41b5;
pub const INVINCIBILITY: u16 = 0x421b;
pub const EXTRA_LIFE: u16 = 0x4357;

pub const DRAGONFLY_L: u16 = 0x4559;
pub const DRAGONFLY_R: u16 = 0x45d1;
pub const FOUNTAIN: u16 = 0x4689;
pub const TWIN_GUN: u16 = 0x4959;
pub const TRANSPORT: u16 = 0x4a2f;
pub const DRONE_R: u16 = 0x4aa7;
pub const DRONE_L: u16 = 0x4b97;
pub const DART: u16 = 0x4c87;

pub const BOSS: u16 = 0x5893;
pub const BOSS_PART_2: u16 = 0x4cbd;
pub const BOSS_PART_3: u16 = 0x507d;
pub const BOSS_PART_4: u16 = 0x53a7;
pub const BOSS_PART_5: u16 = 0x5749;

//! LEVEL_5 (TECHNO): the layout generator and the enemy AI, together.
//!
//! [`layout`] builds the level's enemy/pickup spawn placement from the
//! PRNG-driven script; [`ai`] runs the per-type behaviour for those spawns.
//! Both speak the same sprite vocabulary.

pub mod ai;
pub mod layout;

pub use layout::script;

//! The race levels (2, 4, 6): shared enemy AI.
//!
//! Races read their spawn records straight from the WAD rather than generating
//! a layout, so there is no `layout` submodule here; [`ai`] is the per-type
//! behaviour shared by all three race tracks.

pub mod ai;

//! The level-layout generator.
//!
//! Prototype's generated levels (1, 3, 5, 7) build their scenery layout at load
//! from a PRNG-driven layout script rather than storing it. This module
//! reproduces that faithfully: the engine PRNG, the per-level dispatcher script,
//! and the emitter library that writes the object records. See
//! `reference/formats/level-layout.md` for the disassembly it mirrors.

pub mod prng;

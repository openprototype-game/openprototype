//! Abstract input the core understands.
//!
//! The original front-end reads raw keyboard scancodes in a custom INT 9 ISR
//! (Up `0x48`, Down `0x50`, Enter `0x1C`, Esc `0x01`). The core works one level
//! up from that: the platform translates physical keys into these events, so no
//! core code knows about scancodes or a windowing toolkit.

/// A single key press the core reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    Up,
    Down,
    Enter,
    Esc,
    Char(char),
}

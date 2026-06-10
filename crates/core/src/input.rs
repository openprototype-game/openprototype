//! Abstract input the core understands.
//!
//! The original front-end reads raw keyboard scancodes in a custom INT 9 ISR
//! (Up `0x48`, Down `0x50`, Enter `0x1C`, Esc `0x01`). The core works one level
//! up from that: the platform translates physical keys into these events, so no
//! core code knows about scancodes or a windowing toolkit.
//!
//! Events carry the key transition. Menu-style scenes react to
//! [`KeyEvent::Pressed`] only (see [`KeyEvent::pressed`]); the level scene also
//! tracks [`KeyEvent::Released`] to know which keys are held, the way the
//! original's ISR maintains key-state flags for flight controls.

/// A key the core reacts to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
    /// Fire, held with auto-repeat (the original's Ctrl, ISR flag `0x8157`).
    Ctrl,
    /// Weapon switch, edge-triggered (the original's Shift, flag `0x8158`).
    Shift,
    Char(char),
}

/// A key transition: the platform reports both edges, without auto-repeat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    Pressed(Key),
    Released(Key),
}

impl KeyEvent {
    /// The key if this is a press, for scenes that only react to presses.
    pub fn pressed(self) -> Option<Key> {
        match self {
            KeyEvent::Pressed(key) => Some(key),
            KeyEvent::Released(_) => None,
        }
    }
}

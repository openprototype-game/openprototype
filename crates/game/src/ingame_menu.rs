//! The in-game Esc menu.
//!
//! Esc during play freezes the level and opens this menu over the dimmed
//! playfield (the original's per-tick handler at LEVEL_2 file `0x91bb` sets
//! the freeze and dispatch index 2; the handler at `0x79e2` is the menu).
//! Seven items draw in the level's FONT.RAW (the WAD loads the same sheet the
//! front-end uses and blits its 16x15 glyphs at file `0xb465`); a `>` cursor
//! marks the selection, Up/Down wrap, Enter dispatches, Esc resumes play.
//!
//! LOAD GAME and SAVE GAME open a five-slot picker (rows `GAME 1..5`, drawn
//! bright when the slot's file exists and dim through the playfield's
//! brightness table otherwise, the shared list drawer at file `0x7dd3`).
//! Saving writes any slot; loading needs an occupied one. Both flash a toast
//! (`GAME  SAVED` / `GAME LOADED`) over the bare dimmed playfield for the
//! original's two 0.7-second BIOS waits, then return to the items.
//!
//! GRAPHICS..., JOYSTICK... and VOLUME... are drawn but inert: their option
//! submenus are not ported yet. TODO: wire them once the port grows the
//! matching settings.
//!
//! The menu is pure UI state; the [`LevelScene`](crate::scene::level) owns
//! the freeze, performs the saves and loads it requests, and reports back via
//! [`InGameMenu::saved`]/[`InGameMenu::loaded`].

use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::input::Key;
use prototype_formats::font::Font;

use crate::savegame::SaveGame;
use crate::savestore::{SLOTS, SaveStore};

/// The seven menu items, top to bottom (strings at LEVEL_2 file `0x108e..`).
const ITEMS: [&str; 7] = [
    "NEW GAME",
    "LOAD GAME",
    "SAVE GAME",
    "GRAPHICS...",
    "JOYSTICK...",
    "VOLUME...",
    "QUIT",
];

/// Geometry, from the original's VRAM offsets (80-byte rows, 4 px per byte):
/// items at x 80 from row 10, one 15-row font line apart, the cursor 24 px
/// left of the text; the pickers indent their five slots to x 120 under a
/// title at x 96; toasts sit at x 80, row 30.
const ITEM_X: i32 = 80;
const ITEM_TOP: i32 = 10;
const ROW_STEP: i32 = 15;
const ITEM_CURSOR_X: i32 = 56;
const SLOT_X: i32 = 120;
const SLOT_TOP: i32 = 30;
const SLOT_CURSOR_X: i32 = 100;
const TITLE_X: i32 = 96;
const TITLE_Y: i32 = 10;
const TOAST_X: i32 = 80;
const TOAST_Y: i32 = 30;

/// How long a toast holds, in logic ticks. The original blocks on two 0.7s
/// BIOS waits (`int 15h/86h` at file `0x4d94`, called twice), ~84 ticks.
const TOAST_TICKS: u32 = 84;

/// The save toast really has two spaces (the string at file `0x1276`).
const SAVED_TOAST: &str = "GAME  SAVED";
const LOADED_TOAST: &str = "GAME LOADED";

/// What the scene must do for the menu. Saves and loads come back as
/// requests because only the scene can snapshot or rebuild itself.
pub enum MenuRequest {
    /// Close the menu and resume play.
    Resume,
    /// Restart the chain fresh (the original's exit status 4).
    NewGame,
    /// Back to the front-end menu (exit status 2).
    Quit,
    /// Write the running level into this slot.
    Save(usize),
    /// Load this (occupied) slot.
    Load(usize),
}

enum Screen {
    Items {
        selected: usize,
    },
    Slots {
        saving: bool,
        selected: usize,
        occupied: [bool; SLOTS],
    },
    Toast {
        text: &'static str,
        ticks_left: u32,
    },
}

pub struct InGameMenu {
    screen: Screen,
    /// `None` when the data directory could not be resolved; the pickers
    /// then show every slot empty and saves are dropped with a warning.
    store: Option<SaveStore>,
}

impl InGameMenu {
    pub fn new(store: Option<SaveStore>) -> Self {
        Self {
            screen: Screen::Items { selected: 0 },
            store,
        }
    }

    /// React to a key press, returning what the scene should do. `None`
    /// means the menu handled it internally (or ignored it).
    pub fn handle_key(&mut self, key: Key) -> Option<MenuRequest> {
        match &mut self.screen {
            Screen::Items { selected } => match key {
                Key::Down => {
                    *selected = (*selected + 1) % ITEMS.len();
                    None
                }
                Key::Up => {
                    *selected = (*selected + ITEMS.len() - 1) % ITEMS.len();
                    None
                }
                Key::Esc => Some(MenuRequest::Resume),
                Key::Enter => {
                    let selected = *selected;

                    match selected {
                        0 => Some(MenuRequest::NewGame),
                        1 | 2 => {
                            self.open_slots(selected == 2);
                            None
                        }
                        6 => Some(MenuRequest::Quit),
                        // GRAPHICS.../JOYSTICK.../VOLUME...: drawn, inert.
                        _ => None,
                    }
                }
                _ => None,
            },
            Screen::Slots {
                saving,
                selected,
                occupied,
            } => match key {
                Key::Down => {
                    *selected = (*selected + 1) % SLOTS;
                    None
                }
                Key::Up => {
                    *selected = (*selected + SLOTS - 1) % SLOTS;
                    None
                }
                // Esc backs out to the items, selection reset to the top
                // (the original re-enters the menu through its full redraw).
                Key::Esc => {
                    self.screen = Screen::Items { selected: 0 };
                    None
                }
                Key::Enter => {
                    if *saving {
                        Some(MenuRequest::Save(*selected))
                    } else if occupied[*selected] {
                        Some(MenuRequest::Load(*selected))
                    } else {
                        // An empty slot ignores Enter, like the original.
                        None
                    }
                }
                _ => None,
            },
            // The original blocks in a BIOS wait during the toast.
            Screen::Toast { .. } => None,
        }
    }

    /// Burn toast time; the toast returns to the items when it runs out.
    pub fn advance(&mut self, ticks: u32) {
        if let Screen::Toast { ticks_left, .. } = &mut self.screen {
            *ticks_left = ticks_left.saturating_sub(ticks);

            if *ticks_left == 0 {
                self.screen = Screen::Items { selected: 0 };
            }
        }
    }

    /// Write `save` into `slot` and toast on success. A store failure keeps
    /// the picker open; the player can pick another slot or back out.
    pub fn save_to(&mut self, slot: usize, save: &SaveGame) {
        let Some(store) = &self.store else {
            tracing::warn!("no data directory; the save is dropped");
            return;
        };

        match store.save(slot, save) {
            Ok(()) => self.saved(),
            Err(error) => tracing::warn!("saving slot {}: {error:#}", slot + 1),
        }
    }

    /// Read the save in `slot`.
    pub fn load_from(&self, slot: usize) -> anyhow::Result<SaveGame> {
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no data directory"))?;

        store.load(slot)
    }

    /// Show the save toast.
    pub fn saved(&mut self) {
        self.screen = Screen::Toast {
            text: SAVED_TOAST,
            ticks_left: TOAST_TICKS,
        };
    }

    /// Show the load toast (the scene calls this after an in-place load).
    pub fn loaded(&mut self) {
        self.screen = Screen::Toast {
            text: LOADED_TOAST,
            ticks_left: TOAST_TICKS,
        };
    }

    /// Re-probe the slot files on picker entry, like the original (file
    /// `0xb851` runs at every list draw).
    fn open_slots(&mut self, saving: bool) {
        let occupied = self
            .store
            .as_ref()
            .map(|store| store.occupied())
            .unwrap_or_default();

        self.screen = Screen::Slots {
            saving,
            selected: 0,
            occupied,
        };
    }

    /// Draw the menu over the already-dimmed playfield. `dim` is the level's
    /// brightness table, used for unoccupied slot labels.
    pub fn render(&self, font: &Font, dim: &[u8; 256], frame: &mut Framebuffer) {
        match &self.screen {
            Screen::Items { selected } => {
                for (index, item) in ITEMS.iter().enumerate() {
                    let y = ITEM_TOP + index as i32 * ROW_STEP;
                    font.draw_into(&mut frame.image, ITEM_X, y, item);

                    if index == *selected {
                        font.draw_into(&mut frame.image, ITEM_CURSOR_X, y, ">");
                    }
                }
            }
            Screen::Slots {
                saving,
                selected,
                occupied,
            } => {
                let title = if *saving { "SAVE GAME" } else { "LOAD GAME" };
                font.draw_into(&mut frame.image, TITLE_X, TITLE_Y, title);

                for slot in 0..SLOTS {
                    let y = SLOT_TOP + slot as i32 * ROW_STEP;
                    let label = ["GAME 1", "GAME 2", "GAME 3", "GAME 4", "GAME 5"][slot];

                    if occupied[slot] {
                        font.draw_into(&mut frame.image, SLOT_X, y, label);
                    } else {
                        font.draw_into_mapped(&mut frame.image, SLOT_X, y, label, dim);
                    }

                    if slot == *selected {
                        font.draw_into(&mut frame.image, SLOT_CURSOR_X, y, ">");
                    }
                }
            }
            Screen::Toast { text, .. } => {
                font.draw_into(&mut frame.image, TOAST_X, TOAST_Y, text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A menu without a store: every slot reads empty.
    fn menu() -> InGameMenu {
        InGameMenu::new(None)
    }

    #[test]
    fn the_selection_wraps_both_ways() {
        let mut menu = menu();

        assert!(menu.handle_key(Key::Up).is_none());
        assert!(matches!(menu.screen, Screen::Items { selected: 6 }));

        menu.handle_key(Key::Down);
        assert!(matches!(menu.screen, Screen::Items { selected: 0 }));
    }

    #[test]
    fn enter_on_an_empty_slot_is_ignored_in_the_load_picker() {
        let mut menu = menu();
        menu.handle_key(Key::Down);
        menu.handle_key(Key::Enter);
        assert!(matches!(menu.screen, Screen::Slots { saving: false, .. }));

        assert!(menu.handle_key(Key::Enter).is_none());
        assert!(matches!(menu.screen, Screen::Slots { .. }));
    }

    #[test]
    fn the_save_picker_accepts_any_slot() {
        let mut menu = menu();
        menu.handle_key(Key::Down);
        menu.handle_key(Key::Down);
        menu.handle_key(Key::Enter);
        menu.handle_key(Key::Down);

        assert!(matches!(
            menu.handle_key(Key::Enter),
            Some(MenuRequest::Save(1))
        ));
    }

    #[test]
    fn backing_out_of_a_picker_resets_the_items_selection() {
        let mut menu = menu();
        menu.handle_key(Key::Down);
        menu.handle_key(Key::Enter);
        menu.handle_key(Key::Esc);

        assert!(matches!(menu.screen, Screen::Items { selected: 0 }));
    }

    #[test]
    fn the_toast_blocks_keys_and_returns_to_the_items() {
        let mut menu = menu();
        menu.saved();

        assert!(menu.handle_key(Key::Esc).is_none());

        menu.advance(TOAST_TICKS - 1);
        assert!(matches!(menu.screen, Screen::Toast { .. }));

        menu.advance(1);
        assert!(matches!(menu.screen, Screen::Items { selected: 0 }));
    }
}

//! The main menu.
//!
//! Mirrors `START.EXE`'s menu loop: a [`ListMenu`] over NEW GAME, LOAD GAME,
//! HIGHSCORES, MUSIC MENU, QUIT. Up/Down move the cursor (wrapping); Enter
//! dispatches; every other key, Esc included, is ignored (the original's loop
//! only handles those three scancodes, so quitting is the QUIT item).
//!
//! LOAD GAME swaps the item list for the five-slot picker (file `0x43c7`):
//! GAME 1..5 over the same backdrop, occupied slots bright and empty ones at
//! half brightness, re-probed on every entry. Enter on an occupied slot boots
//! that save (the original reads the slot's level byte, writes the
//! `f:message` mode form `{1, slot, level}`, and launches the level, which
//! loads the snapshot itself); Esc backs out to the items.
//!
//! The menu emits no audio. The title theme is started once at boot, and the
//! original never restarts it from the menu.

use std::rc::Rc;
use std::time::Duration;

use strum::{Display, EnumIter, IntoEnumIterator};

use crate::assets::MenuAssets;
use crate::levels::Level;
use crate::savestore::{SLOTS, SaveStore};
use crate::scene::list_menu::{ListMenu, MenuLayout};
use crate::scene::{Scene, SceneId, SceneOutput, Transition};
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::game_state::Handoff;
use openprototype_core::input::{Key, KeyEvent};

/// The main menu's positions.
///
/// Labels at x=90 (`di = 0x4b5a`, ... in the setup at `0x413e`), the `>`
/// cursor at x=70 (`0x4b46`), first row at y=60.
const LAYOUT: MenuLayout = MenuLayout {
    label_x: 90,
    cursor_x: 70,
    first_row_y: 60,
};

/// The slot picker's positions.
///
/// Labels at x=120 (`di = 0x4b78` in the picker at file `0x43c7`), the cursor
/// at x=100 (`0x4b64`), the same first row.
const SLOT_LAYOUT: MenuLayout = MenuLayout {
    label_x: 120,
    cursor_x: 100,
    first_row_y: 60,
};

/// The main menu's items.
#[derive(Clone, Copy, PartialEq, Eq, EnumIter, Display)]
enum MenuItem {
    #[strum(to_string = "NEW GAME")]
    NewGame,
    #[strum(to_string = "LOAD GAME")]
    LoadGame,
    #[strum(to_string = "HIGHSCORES")]
    Highscores,
    #[strum(to_string = "MUSIC MENU")]
    MusicMenu,
    #[strum(to_string = "QUIT")]
    Quit,
}

impl MenuItem {
    /// The transition Enter on this item triggers.
    ///
    /// `None` while the item has no scene yet.
    fn activate(self) -> Option<Transition> {
        match self {
            MenuItem::NewGame => Some(Transition::To(SceneId::Level {
                level: Level::L1,
                handoff: Handoff::new_game(),
            })),
            // LOAD GAME opens the slot picker instead of transitioning; the
            // scene handles it before calling here.
            MenuItem::LoadGame => None,
            MenuItem::Highscores => Some(Transition::To(SceneId::Highscores)),
            MenuItem::MusicMenu => Some(Transition::To(SceneId::MusicMenu)),
            MenuItem::Quit => Some(Transition::Quit),
        }
    }
}

/// Which list the menu shows: its items, or the LOAD GAME slot picker.
enum State {
    Items,
    LoadSlots { occupied: [bool; SLOTS] },
}

/// The main menu scene.
pub struct Menu {
    assets: Rc<MenuAssets>,
    list: ListMenu,
    state: State,
    save_store: Option<SaveStore>,
}

impl Menu {
    /// Builds the main menu scene.
    pub fn new(assets: Rc<MenuAssets>) -> Self {
        let labels = MenuItem::iter().map(|item| item.to_string()).collect();

        Self {
            list: ListMenu::new(assets.clone(), labels, LAYOUT),
            assets,
            state: State::Items,
            save_store: crate::savestore::open_or_warn(),
        }
    }

    /// Swaps in the slot picker, re-probing the slot files.
    ///
    /// Like the original (file `0x433e` opens all five on every entry).
    fn open_load_slots(&mut self) {
        let occupied = self
            .save_store
            .as_ref()
            .map(|store| store.occupied())
            .unwrap_or_default();

        let labels = (1..=SLOTS).map(|slot| format!("GAME {slot}")).collect();
        let mut list = ListMenu::new(self.assets.clone(), labels, SLOT_LAYOUT);
        list.set_dim_rows(occupied.iter().map(|&occupied| !occupied).collect());

        self.list = list;
        self.state = State::LoadSlots { occupied };
    }

    /// Backs out of the picker to the items, cursor reset.
    ///
    /// The original re-enters the menu through its full redraw.
    fn close_load_slots(&mut self) {
        let labels = MenuItem::iter().map(|item| item.to_string()).collect();
        self.list = ListMenu::new(self.assets.clone(), labels, LAYOUT);
        self.state = State::Items;
    }

    /// The menu frame with the cursor hidden, for the intro's fade-in.
    ///
    /// The menu loop draws the cursor only once it starts, so the fade shows
    /// labels only.
    pub fn frame_without_cursor(&mut self) -> &Framebuffer {
        self.list.render_without_cursor();
        self.list.framebuffer()
    }
}

impl Scene for Menu {
    fn update(&mut self, _dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        for key in input.iter().filter_map(|event| event.pressed()) {
            match key {
                Key::Up => self.list.move_up(),
                Key::Down => self.list.move_down(),
                Key::Enter => match &self.state {
                    State::Items => {
                        let item = MenuItem::iter().nth(self.list.selected());

                        if item == Some(MenuItem::LoadGame) {
                            self.open_load_slots();
                        } else if let Some(item) = item {
                            output.transition = item.activate();
                        }
                    }
                    State::LoadSlots { occupied } => {
                        let slot = self.list.selected();

                        // An empty slot ignores Enter, like the original.
                        if occupied[slot] {
                            output.transition = Some(Transition::To(SceneId::LoadGame { slot }));
                        }
                    }
                },
                // Esc only backs out of the picker; the items ignore it
                // (the original's loop handles no other scancode there).
                Key::Esc => {
                    if matches!(self.state, State::LoadSlots { .. }) {
                        self.close_load_slots();
                    }
                }
                Key::Left | Key::Right | Key::Ctrl | Key::Shift | Key::Backspace | Key::Char(_) => {
                }
            }
        }

        output
    }

    fn framebuffer(&self) -> &Framebuffer {
        self.list.framebuffer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::test_menu_assets;

    fn test_menu() -> Menu {
        Menu::new(Rc::new(test_menu_assets()))
    }

    #[test]
    fn up_wraps_to_last_and_down_wraps_to_first() {
        let mut menu = test_menu();
        let last = MenuItem::iter().count() - 1;

        menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Up)]);
        assert_eq!(menu.list.selected(), last);

        menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Down)]);
        assert_eq!(menu.list.selected(), 0);
    }

    #[test]
    fn enter_on_music_menu_opens_the_jukebox() {
        let mut menu = test_menu();
        // MUSIC MENU is the fourth item (index 3).
        menu.update(
            Duration::ZERO,
            &[
                KeyEvent::Pressed(Key::Down),
                KeyEvent::Pressed(Key::Down),
                KeyEvent::Pressed(Key::Down),
            ],
        );

        assert_eq!(
            menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)])
                .transition,
            Some(Transition::To(SceneId::MusicMenu))
        );
    }

    #[test]
    fn enter_on_quit_requests_exit() {
        let mut menu = test_menu();
        menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Up)]); // QUIT is the last item

        assert_eq!(
            menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)])
                .transition,
            Some(Transition::Quit)
        );
    }

    #[test]
    fn new_game_starts_the_chain_at_level_1() {
        let mut menu = test_menu(); // starts on NEW GAME

        assert_eq!(
            menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)])
                .transition,
            Some(Transition::To(SceneId::Level {
                level: Level::L1,
                handoff: Handoff::new_game(),
            }))
        );
    }

    /// A menu over a temp-dir slot store, with the race fixture saved into
    /// `occupied_slot` when given.
    fn menu_with_store(occupied_slot: Option<usize>) -> (tempfile::TempDir, Menu) {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::savestore::store_at(dir.path().to_path_buf());

        if let Some(slot) = occupied_slot {
            let save = crate::savegame::SaveGame::decode(include_bytes!(
                "../../tests/fixtures/l2-race.psg"
            ))
            .expect("the ground-truth fixture decodes");
            store.save(slot, &save).unwrap();
        }

        let mut menu = test_menu();
        menu.save_store = Some(store);

        (dir, menu)
    }

    /// Navigate from the items into the slot picker (LOAD GAME is the
    /// second item).
    fn open_picker(menu: &mut Menu) {
        menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Down)]);
        menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)]);
    }

    #[test]
    fn load_game_opens_the_picker_and_an_empty_slot_ignores_enter() {
        let (_dir, mut menu) = menu_with_store(None);
        open_picker(&mut menu);

        assert!(matches!(menu.state, State::LoadSlots { .. }));
        assert_eq!(
            menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)])
                .transition,
            None
        );
    }

    #[test]
    fn an_occupied_slot_boots_that_save() {
        let (_dir, mut menu) = menu_with_store(Some(2));
        open_picker(&mut menu);

        menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Down)]);
        menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Down)]);

        assert_eq!(
            menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)])
                .transition,
            Some(Transition::To(SceneId::LoadGame { slot: 2 }))
        );
    }

    #[test]
    fn esc_backs_out_of_the_picker_to_the_items() {
        let (_dir, mut menu) = menu_with_store(None);
        open_picker(&mut menu);

        menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Esc)]);

        assert!(matches!(menu.state, State::Items));
        // The cursor reset to the top: Enter starts a new game.
        assert_eq!(
            menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)])
                .transition,
            Some(Transition::To(SceneId::Level {
                level: Level::L1,
                handoff: Handoff::new_game(),
            }))
        );
    }

    #[test]
    fn esc_is_ignored_like_every_unmapped_key() {
        let mut menu = test_menu();

        assert_eq!(
            menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Esc)])
                .transition,
            None
        );
    }

    #[test]
    fn menu_emits_no_audio() {
        let mut menu = test_menu();

        assert!(menu.update(Duration::ZERO, &[]).audio.is_empty());
    }
}

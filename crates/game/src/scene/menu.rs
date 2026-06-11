//! The main menu.
//!
//! Mirrors `START.EXE`'s menu loop: a [`ListMenu`] over NEW GAME, LOAD GAME,
//! HIGHSCORES, MUSIC MENU, QUIT. Up/Down move the cursor (wrapping); Enter
//! dispatches; every other key, Esc included, is ignored (the original's loop
//! only handles those three scancodes, so quitting is the QUIT item). Only
//! MUSIC MENU and QUIT do something so far; the rest get their scenes later.
//! The menu emits no audio. The title theme is started once at boot, and the
//! original never restarts it from the menu.

use std::rc::Rc;
use std::time::Duration;

use strum::{Display, EnumIter, IntoEnumIterator};

use crate::assets::MenuAssets;
use crate::scene::list_menu::{ListMenu, MenuLayout};
use crate::scene::{Scene, SceneId, SceneOutput, Transition};
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::input::{Key, KeyEvent};

/// The main menu's positions: labels at x=90 (`di = 0x4b5a`, ... in the setup
/// at `0x413e`), the `>` cursor at x=70 (`0x4b46`), first row at y=60.
const LAYOUT: MenuLayout = MenuLayout {
    label_x: 90,
    cursor_x: 70,
    first_row_y: 60,
};

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
    /// The transition Enter on this item triggers, or `None` while the item has
    /// no scene yet.
    fn activate(self) -> Option<Transition> {
        match self {
            MenuItem::NewGame => None,
            MenuItem::LoadGame => None,
            MenuItem::Highscores => Some(Transition::To(SceneId::Highscores)),
            MenuItem::MusicMenu => Some(Transition::To(SceneId::MusicMenu)),
            MenuItem::Quit => Some(Transition::Quit),
        }
    }
}

pub struct Menu {
    list: ListMenu,
}

impl Menu {
    pub fn new(assets: Rc<MenuAssets>) -> Self {
        let labels = MenuItem::iter().map(|item| item.to_string()).collect();

        Self {
            list: ListMenu::new(assets, labels, LAYOUT),
        }
    }

    /// The menu frame with the cursor hidden, for the intro's fade-in. The menu
    /// loop draws the cursor only once it starts, so the fade shows labels only.
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
                Key::Enter => {
                    if let Some(item) = MenuItem::iter().nth(self.list.selected()) {
                        output.transition = item.activate();
                    }
                }
                Key::Esc | Key::Left | Key::Right | Key::Ctrl | Key::Shift | Key::Char(_) => {}
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
    fn enter_on_an_unwired_item_does_nothing() {
        let mut menu = test_menu(); // starts on NEW GAME

        assert_eq!(
            menu.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)])
                .transition,
            None
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

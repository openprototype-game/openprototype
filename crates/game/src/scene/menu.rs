//! The main menu.
//!
//! Mirrors `START.EXE`'s menu loop: a [`ListMenu`] over NEW GAME, LOAD GAME,
//! HIGHSCORES, MUSIC MENU, QUIT. Up/Down move the cursor (wrapping); Enter
//! dispatches; Esc quits. In this shell only QUIT does something real; the rest
//! get their scenes later. The menu emits no audio. The title theme is started
//! once at boot, and the original never restarts it from the menu.

use std::rc::Rc;

use strum::{Display, EnumIter, IntoEnumIterator};

use crate::assets::MenuAssets;
use crate::core::framebuffer::Framebuffer;
use crate::core::input::KeyEvent;
use crate::scene::list_menu::ListMenu;
use crate::scene::{Scene, SceneOutput, Transition};

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
            MenuItem::Highscores => None,
            MenuItem::MusicMenu => None,
            MenuItem::Quit => Some(Transition::Quit),
        }
    }
}

pub struct Menu {
    assets: Rc<MenuAssets>,
    framebuffer: Framebuffer,
    list: ListMenu,
    labels: Vec<String>,
}

impl Menu {
    pub fn new(assets: Rc<MenuAssets>) -> Self {
        let labels: Vec<String> = MenuItem::iter().map(|item| item.to_string()).collect();
        let framebuffer = Framebuffer::new(assets.palette.clone());
        let list = ListMenu::new(labels.len());

        let mut menu = Self {
            assets,
            framebuffer,
            list,
            labels,
        };

        menu.render();
        menu
    }

    fn render(&mut self) {
        let labels: Vec<&str> = self.labels.iter().map(String::as_str).collect();
        self.list
            .render(&mut self.framebuffer, &self.assets, &labels);
    }
}

impl Scene for Menu {
    fn update(&mut self, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        for event in input {
            match event {
                KeyEvent::Up => self.list.move_up(),
                KeyEvent::Down => self.list.move_down(),
                KeyEvent::Esc => output.transition = Some(Transition::Quit),
                KeyEvent::Enter => {
                    if let Some(item) = MenuItem::iter().nth(self.list.selected()) {
                        output.transition = item.activate();
                    }
                }
                KeyEvent::Char(_) => {}
            }
        }

        self.render();
        output
    }

    fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
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

        menu.update(&[KeyEvent::Up]);
        assert_eq!(menu.list.selected(), last);

        menu.update(&[KeyEvent::Down]);
        assert_eq!(menu.list.selected(), 0);
    }

    #[test]
    fn enter_on_quit_requests_exit() {
        let mut menu = test_menu();
        menu.update(&[KeyEvent::Up]); // QUIT is the last item

        assert_eq!(
            menu.update(&[KeyEvent::Enter]).transition,
            Some(Transition::Quit)
        );
    }

    #[test]
    fn enter_on_an_unwired_item_does_nothing() {
        let mut menu = test_menu(); // starts on NEW GAME

        assert_eq!(menu.update(&[KeyEvent::Enter]).transition, None);
    }

    #[test]
    fn esc_requests_exit() {
        let mut menu = test_menu();

        assert_eq!(
            menu.update(&[KeyEvent::Esc]).transition,
            Some(Transition::Quit)
        );
    }

    #[test]
    fn menu_emits_no_audio() {
        let mut menu = test_menu();

        assert!(menu.update(&[]).audio.is_empty());
    }
}

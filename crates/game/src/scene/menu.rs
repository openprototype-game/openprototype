//! The main menu.
//!
//! Mirrors `START.EXE`'s menu loop (`0x3e41`): blit `BACK3.RAW`, draw five
//! labels with the glyph blitter at x=90, and a `>` cursor at x=70 on the
//! active row. Rows sit at y = 60, 76, 92, 108, 124 (16 scanlines apart). Up
//! and Down move the cursor with wraparound; Enter dispatches.
//!
//! In this shell only QUIT does something real (it asks the platform to exit).
//! The other items get their own scenes later; for now they are inert. Track 2
//! starts on the first frame so the menu has the music the original plays from
//! the intro onward.

use crate::assets::MenuAssets;
use crate::core::audio::AudioCommand;
use crate::core::framebuffer::Framebuffer;
use crate::core::game::{Game, StepOutput};
use crate::core::input::KeyEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuItem {
    NewGame,
    LoadGame,
    Highscores,
    MusicMenu,
    Quit,
}

const ITEMS: [(MenuItem, &str); 5] = [
    (MenuItem::NewGame, "NEW GAME"),
    (MenuItem::LoadGame, "LOAD GAME"),
    (MenuItem::Highscores, "HIGHSCORES"),
    (MenuItem::MusicMenu, "MUSIC MENU"),
    (MenuItem::Quit, "QUIT"),
];

/// The CD-DA track the menu plays (the original carries it over from the intro).
const MENU_TRACK: u8 = 2;

const LABEL_X: i32 = 90;
const CURSOR_X: i32 = 70;
const FIRST_ROW_Y: i32 = 60;
const ROW_HEIGHT: i32 = 16;

fn row_y(index: usize) -> i32 {
    FIRST_ROW_Y + index as i32 * ROW_HEIGHT
}

pub struct Menu {
    assets: MenuAssets,
    framebuffer: Framebuffer,
    selected: usize,
    started_music: bool,
}

impl Menu {
    /// Build the menu from its decoded assets and render the initial frame.
    pub fn new(assets: MenuAssets) -> Self {
        let framebuffer = Framebuffer::new(assets.palette.clone());
        let mut menu = Self {
            assets,
            framebuffer,
            selected: 0,
            started_music: false,
        };

        menu.render();
        menu
    }

    fn move_cursor(&mut self, delta: i32) {
        let count = ITEMS.len() as i32;
        self.selected = (self.selected as i32 + delta).rem_euclid(count) as usize;
    }

    fn render(&mut self) {
        self.framebuffer.blit_screen(&self.assets.background);

        for (index, (_, label)) in ITEMS.iter().enumerate() {
            self.assets
                .font
                .draw_into(&mut self.framebuffer.image, LABEL_X, row_y(index), label);
        }

        self.assets.font.draw_into(
            &mut self.framebuffer.image,
            CURSOR_X,
            row_y(self.selected),
            ">",
        );
    }
}

impl Game for Menu {
    fn step(&mut self, input: &[KeyEvent]) -> StepOutput {
        let mut output = StepOutput::default();

        if !self.started_music {
            output.audio.push(AudioCommand::PlayTrack(MENU_TRACK));
            self.started_music = true;
        }

        for event in input {
            match event {
                KeyEvent::Up => self.move_cursor(-1),
                KeyEvent::Down => self.move_cursor(1),
                KeyEvent::Esc => output.quit = true,
                KeyEvent::Enter => {
                    if ITEMS[self.selected].0 == MenuItem::Quit {
                        output.quit = true;
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
    use crate::core::framebuffer::{SCREEN_HEIGHT, SCREEN_WIDTH};
    use prototype_formats::font::Font;
    use prototype_formats::{Dimensions, IndexedImage, Palette};

    /// A menu backed by blank assets: enough to exercise state and side
    /// effects without touching the disc.
    fn test_menu() -> Menu {
        let background = IndexedImage::new(
            Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
            vec![0u8; (SCREEN_WIDTH * SCREEN_HEIGHT) as usize],
        )
        .unwrap();
        let font_sheet = vec![0u8; 320 * 62];
        let font = Font::decode(&font_sheet).unwrap();
        let palette = Palette::from_vga_6bit(&[0u8; 768]).unwrap();

        Menu::new(MenuAssets {
            background,
            font,
            palette,
        })
    }

    #[test]
    fn down_advances_then_wraps_to_first() {
        let mut menu = test_menu();

        menu.step(&[KeyEvent::Down]);
        assert_eq!(menu.selected, 1);

        menu.selected = ITEMS.len() - 1; // QUIT
        menu.step(&[KeyEvent::Down]);
        assert_eq!(menu.selected, 0);
    }

    #[test]
    fn up_from_first_wraps_to_last() {
        let mut menu = test_menu();

        menu.step(&[KeyEvent::Up]);
        assert_eq!(menu.selected, ITEMS.len() - 1);
    }

    #[test]
    fn enter_on_quit_requests_exit() {
        let mut menu = test_menu();
        menu.selected = ITEMS.len() - 1; // QUIT

        assert!(menu.step(&[KeyEvent::Enter]).quit);
    }

    #[test]
    fn enter_on_other_item_does_not_exit() {
        let mut menu = test_menu();
        menu.selected = 0; // NEW GAME

        assert!(!menu.step(&[KeyEvent::Enter]).quit);
    }

    #[test]
    fn esc_requests_exit() {
        let mut menu = test_menu();

        assert!(menu.step(&[KeyEvent::Esc]).quit);
    }

    #[test]
    fn music_starts_once() {
        let mut menu = test_menu();

        assert_eq!(
            menu.step(&[]).audio,
            vec![AudioCommand::PlayTrack(MENU_TRACK)]
        );
        assert!(menu.step(&[]).audio.is_empty());
    }
}

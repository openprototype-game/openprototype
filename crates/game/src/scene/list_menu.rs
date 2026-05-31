//! A cursor-driven list menu: a background, a column of labels, and a `>`
//! cursor on the selected row, with wraparound navigation.
//!
//! Both the main menu and the jukebox are this widget; they differ only in
//! their labels and what Enter does. It owns the framebuffer and re-renders on
//! navigation, so a scene built on it is just its dispatch logic. Layout mirrors
//! `START.EXE`'s menu loop: labels at x=90, the cursor at x=70, rows at y = 60,
//! 76, 92, ... (16 scanlines apart).

use std::rc::Rc;

use crate::assets::MenuAssets;
use crate::core::framebuffer::Framebuffer;

const LABEL_X: i32 = 90;
const CURSOR_X: i32 = 70;
const FIRST_ROW_Y: i32 = 60;
const ROW_HEIGHT: i32 = 16;
const CURSOR_GLYPH: &str = ">";

fn row_y(index: usize) -> i32 {
    FIRST_ROW_Y + index as i32 * ROW_HEIGHT
}

/// A rendered cursor list over `labels` (which must be non-empty).
pub struct ListMenu {
    assets: Rc<MenuAssets>,
    framebuffer: Framebuffer,
    labels: Vec<String>,
    selected: usize,
}

impl ListMenu {
    pub fn new(assets: Rc<MenuAssets>, labels: Vec<String>) -> Self {
        debug_assert!(!labels.is_empty(), "a list menu needs at least one row");

        let framebuffer = Framebuffer::new(assets.palette.clone());
        let mut menu = Self {
            assets,
            framebuffer,
            labels,
            selected: 0,
        };

        menu.render();
        menu
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    pub fn move_up(&mut self) {
        self.selected = (self.selected + self.labels.len() - 1) % self.labels.len();
        self.render();
    }

    pub fn move_down(&mut self) {
        self.selected = (self.selected + 1) % self.labels.len();
        self.render();
    }

    fn render(&mut self) {
        self.framebuffer.blit_screen(&self.assets.background);

        for (index, label) in self.labels.iter().enumerate() {
            self.assets
                .font
                .draw_into(&mut self.framebuffer.image, LABEL_X, row_y(index), label);
        }

        self.assets.font.draw_into(
            &mut self.framebuffer.image,
            CURSOR_X,
            row_y(self.selected),
            CURSOR_GLYPH,
        );
    }
}

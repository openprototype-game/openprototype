//! A cursor-driven list menu: a background, a column of labels, and a `>`
//! cursor on the selected row, with wraparound navigation.
//!
//! Both the main menu and the jukebox are this widget; they differ in their
//! labels, their [`MenuLayout`], and what Enter does. It owns the framebuffer
//! and re-renders on navigation, so a scene built on it is just its dispatch
//! logic. Rows step 16 scanlines like the original's cursor moves
//! (`di += 0x1400`); the column positions come from each screen's draw calls.

use std::rc::Rc;

use prototype_formats::Dimensions;

use crate::assets::MenuAssets;
use crate::screen::{SCREEN_HEIGHT, SCREEN_WIDTH};
use openprototype_core::framebuffer::Framebuffer;

const ROW_HEIGHT: i32 = 16;
const CURSOR_GLYPH: &str = ">";

/// Where a list menu draws: the label and cursor columns and the first row.
/// The main menu and the jukebox use different positions in `START.EXE`.
pub struct MenuLayout {
    pub label_x: i32,
    pub cursor_x: i32,
    pub first_row_y: i32,
}

/// A rendered cursor list over `labels` (which must be non-empty).
pub struct ListMenu {
    assets: Rc<MenuAssets>,
    framebuffer: Framebuffer,
    labels: Vec<String>,
    layout: MenuLayout,
    selected: usize,
}

impl ListMenu {
    pub fn new(assets: Rc<MenuAssets>, labels: Vec<String>, layout: MenuLayout) -> Self {
        debug_assert!(!labels.is_empty(), "a list menu needs at least one row");

        let framebuffer = Framebuffer::new(
            Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
            assets.palette.clone(),
        );
        let mut menu = Self {
            assets,
            framebuffer,
            labels,
            layout,
            selected: 0,
        };

        menu.render();
        menu
    }

    fn row_y(&self, index: usize) -> i32 {
        self.layout.first_row_y + index as i32 * ROW_HEIGHT
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

    /// Draw the background and labels without the cursor. The intro fades this
    /// in before the menu loop starts; the original draws the cursor only once
    /// the loop runs.
    pub fn render_without_cursor(&mut self) {
        self.framebuffer.blit_screen(&self.assets.background);

        for (index, label) in self.labels.iter().enumerate() {
            let y = self.row_y(index);
            self.assets
                .font
                .draw_into(&mut self.framebuffer.image, self.layout.label_x, y, label);
        }
    }

    fn render(&mut self) {
        self.render_without_cursor();

        let y = self.row_y(self.selected);
        self.assets.font.draw_into(
            &mut self.framebuffer.image,
            self.layout.cursor_x,
            y,
            CURSOR_GLYPH,
        );
    }
}

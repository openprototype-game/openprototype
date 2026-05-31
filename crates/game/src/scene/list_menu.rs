//! A cursor-driven list menu: a background, a column of labels, and a `>`
//! cursor on the selected row, with wraparound navigation.
//!
//! Both the main menu and the jukebox are this widget; they differ only in
//! their labels and what Enter does. Layout mirrors `START.EXE`'s menu loop:
//! labels at x=90, the cursor at x=70, rows at y = 60, 76, 92, ... (16
//! scanlines apart).

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

/// Cursor state over a list of `count` rows (`count` must be non-zero).
pub struct ListMenu {
    selected: usize,
    count: usize,
}

impl ListMenu {
    pub fn new(count: usize) -> Self {
        debug_assert!(count > 0, "a list menu needs at least one row");
        Self { selected: 0, count }
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn move_up(&mut self) {
        self.selected = (self.selected + self.count - 1) % self.count;
    }

    pub fn move_down(&mut self) {
        self.selected = (self.selected + 1) % self.count;
    }

    /// Redraw the background, the `labels` (one per row), and the cursor on the
    /// selected row.
    pub fn render(&self, framebuffer: &mut Framebuffer, assets: &MenuAssets, labels: &[&str]) {
        framebuffer.blit_screen(&assets.background);

        for (index, label) in labels.iter().enumerate() {
            assets
                .font
                .draw_into(&mut framebuffer.image, LABEL_X, row_y(index), label);
        }

        assets.font.draw_into(
            &mut framebuffer.image,
            CURSOR_X,
            row_y(self.selected),
            CURSOR_GLYPH,
        );
    }
}

//! The high-score name entry.
//!
//! Mirrors `START.EXE`'s qualify path inside `0x3f0c`: a black screen with the
//! boot palette, `"  CONGRATULATIONS=  "` at y 65 and `"  ENTER YOUR NAME   "`
//! at y 82 (both `FONT.RAW`, full-width padded at x 0), and the 13-character
//! name field at x 48, y 110, dot-filled. A–Z and space type (lowercase is
//! uppercased), backspace edits, and Enter or Esc both confirm — the original
//! has no cancel. The entry then inserts into the table (just below every
//! score it doesn't beat), the table persists, and the flow falls into the
//! high-score view, which shows the new table and returns to the menu.

use std::rc::Rc;
use std::time::Duration;

use prototype_formats::{Dimensions, Highscore};

use crate::assets::MenuAssets;
use crate::highscores::HighscoreStore;
use crate::scene::{Scene, SceneId, SceneOutput, Transition};
use crate::screen::{SCREEN_HEIGHT, SCREEN_WIDTH};
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::input::{Key, KeyEvent};

/// The two headline strings, exactly as `START.EXE` stores them (`cs:0x562`
/// and `cs:0x577`; the `=` is a glyph in the sheet, not a typo).
const CONGRATULATIONS: &str = "  CONGRATULATIONS=  ";
const ENTER_YOUR_NAME: &str = "  ENTER YOUR NAME   ";

/// Screen positions: the headlines at x 0, y 65/82 (`di` 0x5140/0x6680), the
/// name field at x 48, y 110 (`di` 0x89b0).
const HEADLINE_Y: i32 = 65;
const PROMPT_Y: i32 = 82;
const NAME_X: i32 = 48;
const NAME_Y: i32 = 110;

/// The name buffer holds up to 13 characters (`cs:0x5cf`), dot-padded.
const NAME_CAPACITY: usize = 13;

pub struct HighscoreEntry {
    assets: Rc<MenuAssets>,
    store: Rc<HighscoreStore>,
    score: u32,
    name: String,
    framebuffer: Framebuffer,
}

impl HighscoreEntry {
    pub fn new(assets: Rc<MenuAssets>, store: Rc<HighscoreStore>, score: u32) -> Self {
        let framebuffer = Framebuffer::new(
            Dimensions::new(SCREEN_WIDTH, SCREEN_HEIGHT),
            assets.palette.clone(),
        );

        let mut scene = Self {
            assets,
            store,
            score,
            name: String::new(),
            framebuffer,
        };
        scene.render();

        scene
    }

    fn render(&mut self) {
        self.framebuffer.image.pixels.fill(0);

        let font = &self.assets.font;
        font.draw_into(&mut self.framebuffer.image, 0, HEADLINE_Y, CONGRATULATIONS);
        font.draw_into(&mut self.framebuffer.image, 0, PROMPT_Y, ENTER_YOUR_NAME);

        let field = format!("{:.<NAME_CAPACITY$}", self.name);
        font.draw_into(&mut self.framebuffer.image, NAME_X, NAME_Y, &field);
    }

    fn confirm(&self) -> Transition {
        let mut scores = self.store.load();
        scores.add(Highscore {
            name: self.name.clone(),
            score: self.score,
        });

        if let Err(error) = self.store.save(&scores) {
            tracing::warn!(%error, "saving the high-score table failed");
        }

        Transition::To(SceneId::Highscores)
    }
}

impl Scene for HighscoreEntry {
    fn update(&mut self, _dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();
        let mut edited = false;

        for key in input.iter().filter_map(|event| event.pressed()) {
            match key {
                Key::Enter | Key::Esc => {
                    output.transition = Some(self.confirm());

                    return output;
                }
                Key::Backspace => {
                    edited |= self.name.pop().is_some();
                }
                Key::Char(c)
                    if (c == ' ' || c.is_ascii_alphabetic()) && self.name.len() < NAME_CAPACITY =>
                {
                    self.name.push(c.to_ascii_uppercase());
                    edited = true;
                }
                _ => {}
            }
        }

        if edited {
            self.render();
        }

        output
    }

    fn framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }
}

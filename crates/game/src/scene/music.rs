//! The music menu (jukebox).
//!
//! Mirrors `START.EXE`'s jukebox (`0x439a`): a [`ListMenu`] over MUSIC 1..7.
//! Enter plays the selected track (MUSIC N is CD track N+1, so MUSIC 1 is track
//! 2, the title theme); Esc returns to the main menu. Like the original, a
//! selection plays the track once and the music keeps playing on return; the
//! platform owns playback, so leaving the scene does not stop it.

use std::rc::Rc;
use std::time::Duration;

use crate::assets::MenuAssets;
use crate::scene::list_menu::ListMenu;
use crate::scene::{Scene, SceneId, SceneOutput, Transition};
use openprototype_core::audio::AudioCommand;
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::input::{Key, KeyEvent};

/// Number of songs (MUSIC 1..=7).
const TRACK_COUNT: usize = 7;
/// CD track of MUSIC 1 (track 1 is data; the songs are tracks 2..=8).
const FIRST_MUSIC_TRACK: u8 = 2;

pub struct MusicMenu {
    list: ListMenu,
}

impl MusicMenu {
    pub fn new(assets: Rc<MenuAssets>) -> Self {
        let labels = (1..=TRACK_COUNT).map(|n| format!("MUSIC {n}")).collect();

        Self {
            list: ListMenu::new(assets, labels),
        }
    }
}

impl Scene for MusicMenu {
    fn update(&mut self, _dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        for key in input.iter().filter_map(|event| event.pressed()) {
            match key {
                Key::Up => self.list.move_up(),
                Key::Down => self.list.move_down(),
                Key::Esc => output.transition = Some(Transition::To(SceneId::MainMenu)),
                Key::Enter => {
                    let track = FIRST_MUSIC_TRACK + self.list.selected() as u8;
                    output.audio.push(AudioCommand::PlayTrack(track));
                }
                Key::Left | Key::Right | Key::Ctrl | Key::Shift | Key::Char(_) => {}
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

    fn test_jukebox() -> MusicMenu {
        MusicMenu::new(Rc::new(test_menu_assets()))
    }

    #[test]
    fn enter_on_music_1_plays_the_title_theme() {
        let mut jukebox = test_jukebox(); // starts on MUSIC 1

        assert_eq!(
            jukebox
                .update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)])
                .audio,
            vec![AudioCommand::PlayTrack(2)]
        );
    }

    #[test]
    fn enter_on_music_7_plays_the_last_track() {
        let mut jukebox = test_jukebox();
        jukebox.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Up)]); // MUSIC 7 is the last entry (wrap up)

        assert_eq!(
            jukebox
                .update(Duration::ZERO, &[KeyEvent::Pressed(Key::Enter)])
                .audio,
            vec![AudioCommand::PlayTrack(8)]
        );
    }

    #[test]
    fn esc_returns_to_the_main_menu() {
        let mut jukebox = test_jukebox();

        assert_eq!(
            jukebox
                .update(Duration::ZERO, &[KeyEvent::Pressed(Key::Esc)])
                .transition,
            Some(Transition::To(SceneId::MainMenu))
        );
    }

    #[test]
    fn navigation_does_not_play_or_transition() {
        let mut jukebox = test_jukebox();
        let output = jukebox.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Down)]);

        assert!(output.audio.is_empty());
        assert_eq!(output.transition, None);
    }
}

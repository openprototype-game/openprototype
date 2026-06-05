//! A developer scene for live-testing the in-game level render.
//!
//! Not part of the normal front-end flow: the `--scene level` flag boots
//! straight into it. It composites a still of the canyon background, the HUD
//! panel, and the animated weapon pod into one 320x240 frame, so the panel
//! geometry and the pod's open/settle animation can be checked against footage.
//!
//! All four secondaries start fully charged. Enter cycles the selected weapon
//! (replaying the pod animation), Up/Down nudge the panel's top row so it can be
//! pinned live, and Esc quits. The scrolling parallax and the over-panel overlay
//! sprite are later passes; this shows a fixed background window.

use std::rc::Rc;
use std::time::Duration;

use prototype_formats::Dimensions;

use crate::assets::LevelAssets;
use crate::hud::{self, POD_SETTLED_FRAME};
use crate::scene::{Scene, SceneOutput, Transition};
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::input::KeyEvent;
use openprototype_core::{GameState, Lives, Secondary, SmartBombs, WeaponLevel};

/// The level's Mode X frame: 320x240, native 4:3.
const SCREEN: Dimensions = Dimensions {
    width: 320,
    height: 240,
};

/// Which canyon row sits at the top of the playfield. A fixed window stands in
/// for the scrolling parallax.
const BACKGROUND_TOP: i32 = 0;

/// How long each pod animation frame holds while the pod opens and settles.
const POD_FRAME_DURATION: Duration = Duration::from_millis(70);

pub struct LevelScene {
    assets: Rc<LevelAssets>,
    state: GameState,
    frame: Framebuffer,
    /// Screen row of the panel's top edge, nudged live with Up/Down.
    panel_top: i32,
    /// The pod's current animation frame, `0` (hidden) up to [`POD_SETTLED_FRAME`].
    pod_frame: usize,
    pod_elapsed: Duration,
}

impl LevelScene {
    pub fn new(assets: Rc<LevelAssets>) -> Self {
        let state = GameState {
            score: 0,
            lives: Lives::new(3),
            smart_bombs: SmartBombs::new(3),
            weapons: [WeaponLevel::new(WeaponLevel::MAX); 4],
            selected: Secondary::One,
            invincible_ticks: 0,
        };

        eprintln!("level scene: Enter cycles weapon, Up/Down nudge the panel, Esc quits");

        let frame = Framebuffer::new(SCREEN, assets.hud.palette.clone());
        let mut scene = Self {
            assets,
            state,
            frame,
            panel_top: hud::PANEL_TOP,
            pod_frame: POD_SETTLED_FRAME,
            pod_elapsed: Duration::ZERO,
        };
        scene.render();

        scene
    }

    /// Cycle to the next secondary and replay the pod's open/settle animation.
    fn cycle_weapon(&mut self) {
        self.state.cycle_weapon();
        self.pod_frame = 0;
        self.pod_elapsed = Duration::ZERO;
    }

    /// Move the panel's top edge by `delta` rows and report the new value, so it
    /// can be pinned against real footage.
    fn nudge_panel(&mut self, delta: i32) {
        self.panel_top = (self.panel_top + delta).clamp(0, SCREEN.height as i32 - 1);
        eprintln!("PANEL_TOP = {}", self.panel_top);
    }

    /// Step the pod animation toward the settled frame.
    fn advance_pod(&mut self, dt: Duration) {
        self.pod_elapsed += dt;

        while self.pod_frame < POD_SETTLED_FRAME && self.pod_elapsed >= POD_FRAME_DURATION {
            self.pod_elapsed -= POD_FRAME_DURATION;
            self.pod_frame += 1;
        }
    }

    /// Composite the background, HUD, and animated pod into the frame.
    fn render(&mut self) {
        self.frame.blit(&self.assets.background, 0, -BACKGROUND_TOP);
        hud::draw_hud(
            &self.state,
            &self.assets.hud,
            self.panel_top,
            &mut self.frame,
        );
        hud::draw_weapon_pod(
            self.state.firing_weapon(),
            self.pod_frame,
            &self.assets.hud,
            self.panel_top,
            &mut self.frame,
        );
    }
}

impl Scene for LevelScene {
    fn update(&mut self, dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        for event in input {
            match event {
                KeyEvent::Enter => self.cycle_weapon(),
                KeyEvent::Up => self.nudge_panel(-1),
                KeyEvent::Down => self.nudge_panel(1),
                KeyEvent::Esc => output.transition = Some(Transition::Quit),
                KeyEvent::Char(_) => {}
            }
        }

        self.advance_pod(dt);
        self.render();

        output
    }

    fn framebuffer(&self) -> &Framebuffer {
        &self.frame
    }

    fn is_animating(&self) -> bool {
        self.pod_frame < POD_SETTLED_FRAME
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::test_level_assets;
    use openprototype_core::Weapon;

    fn test_scene() -> LevelScene {
        LevelScene::new(Rc::new(test_level_assets()))
    }

    #[test]
    fn starts_with_all_secondaries_charged_and_the_pod_settled() {
        let scene = test_scene();

        for secondary in Secondary::ALL {
            assert_eq!(scene.state.level(secondary).get(), WeaponLevel::MAX);
        }

        assert_eq!(scene.pod_frame, POD_SETTLED_FRAME);
        assert!(!scene.is_animating());
    }

    #[test]
    fn enter_cycles_the_weapon_and_restarts_the_pod_animation() {
        let mut scene = test_scene();
        assert_eq!(scene.state.firing_weapon(), Weapon::Secondary1);

        scene.update(Duration::ZERO, &[KeyEvent::Enter]);

        assert_eq!(scene.state.firing_weapon(), Weapon::Secondary2);
        assert_eq!(scene.pod_frame, 0);
        assert!(scene.is_animating());
    }

    #[test]
    fn the_pod_animation_advances_to_settled_then_stops() {
        let mut scene = test_scene();
        scene.update(Duration::ZERO, &[KeyEvent::Enter]);

        // Five frame-durations carry frame 0 up to the settled frame.
        for _ in 0..POD_SETTLED_FRAME {
            scene.update(POD_FRAME_DURATION, &[]);
        }

        assert_eq!(scene.pod_frame, POD_SETTLED_FRAME);
        assert!(!scene.is_animating());
    }

    #[test]
    fn up_and_down_nudge_the_panel_within_the_frame() {
        let mut scene = test_scene();
        let start = scene.panel_top;

        scene.update(Duration::ZERO, &[KeyEvent::Up]);
        assert_eq!(scene.panel_top, start - 1);

        scene.update(Duration::ZERO, &[KeyEvent::Down, KeyEvent::Down]);
        assert_eq!(scene.panel_top, start + 1);
    }

    #[test]
    fn esc_quits() {
        let mut scene = test_scene();

        assert_eq!(
            scene.update(Duration::ZERO, &[KeyEvent::Esc]).transition,
            Some(Transition::Quit)
        );
    }
}

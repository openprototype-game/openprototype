//! A developer scene for live-testing the in-game level render.
//!
//! Not part of the normal front-end flow: the `--scene level` flag boots
//! straight into it. It composites a still of the canyon background, the HUD
//! panel, and the animated weapon pod into one 320x240 frame, so the panel
//! geometry and the pod's open/settle animation can be checked against footage.
//!
//! All four secondaries start fully charged. Enter cycles the selected weapon
//! (replaying the pod and overlay animations), Up/Down nudge the panel's top row
//! and WASD the overlay, so both can be pinned live against footage, and Esc
//! quits. The scrolling parallax is a later pass; this shows a fixed background
//! window.

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
///
/// TODO: 70ms is an unverified placeholder, picked so the animation is visible
/// during dev. The faithful rate is not yet traced: it depends on the main
/// loop's frame rate (Mode X 320x240 double-scanned to 480 lines is 60Hz if the
/// loop is vsync-locked) and the divider on the anim counter (`cs:0x2699`). Pin
/// it with a write-breakpoint on the counter, counting vsyncs between steps.
const POD_FRAME_DURATION: Duration = Duration::from_millis(70);

/// The overlay's x. Pinned live against footage; it lands on the weapon pod's
/// column (the pod draws at screen x 252, `di` 0x3f), so the cut-off weapon top
/// sits directly above its pod. Still nudgeable with A/D.
const OVERLAY_X: i32 = 251;

/// The overlay's top, as rows above [`panel_top`](LevelScene::panel_top). Pinned
/// live: `-7` is the overlay's own height, so its bottom edge meets the panel's
/// top row and the cut-off top extends up from there. Still nudgeable with W/S.
const OVERLAY_OFFSET_Y: i32 = -7;

pub struct LevelScene {
    assets: Rc<LevelAssets>,
    state: GameState,
    frame: Framebuffer,
    /// Screen row of the panel's top edge, nudged live with Up/Down.
    panel_top: i32,
    /// The overlay's screen x, nudged live with A/D.
    overlay_x: i32,
    /// The overlay's top relative to [`panel_top`](Self::panel_top), nudged with W/S.
    overlay_offset_y: i32,
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

        eprintln!(
            "level scene: Enter cycles weapon, Up/Down nudge the panel, \
             WASD nudge the overlay, Esc quits"
        );

        let frame = Framebuffer::new(SCREEN, assets.hud.palette.clone());
        let mut scene = Self {
            assets,
            state,
            frame,
            panel_top: hud::PANEL_TOP,
            overlay_x: OVERLAY_X,
            overlay_offset_y: OVERLAY_OFFSET_Y,
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

    /// Move the overlay by `(dx, dy)` and report its position, to pin it live.
    fn nudge_overlay(&mut self, dx: i32, dy: i32) {
        self.overlay_x += dx;
        self.overlay_offset_y += dy;
        eprintln!(
            "overlay x = {}, y = panel_top {:+}",
            self.overlay_x, self.overlay_offset_y
        );
    }

    /// Step the pod animation toward the settled frame.
    fn advance_pod(&mut self, dt: Duration) {
        self.pod_elapsed += dt;

        while self.pod_frame < POD_SETTLED_FRAME && self.pod_elapsed >= POD_FRAME_DURATION {
            self.pod_elapsed -= POD_FRAME_DURATION;
            self.pod_frame += 1;
        }
    }

    /// Composite the background, weapon overlay, HUD, and animated pod.
    ///
    /// The overlay is a playfield sprite, drawn before the panel so the opaque
    /// `PANEL.RAW` masks its lower rows. While the pod opens its slide keeps it
    /// at the panel's top edge (hidden behind the panel); it only clears the
    /// panel once it snaps up to its settled row. The original gates the
    /// playfield sprite blitter against the HUD band for the same effect.
    fn render(&mut self) {
        let firing = self.state.firing_weapon();

        self.frame.blit(&self.assets.background, 0, -BACKGROUND_TOP);

        let overlay = &self.assets.overlays[firing as usize];
        let slide = &self.assets.overlay_slide[firing as usize];
        let (slide_x, slide_y) = slide[self.pod_frame.min(slide.len() - 1)];
        self.frame.blit_transparent(
            &overlay.pixels,
            overlay.size,
            self.overlay_x + slide_x,
            self.panel_top + self.overlay_offset_y + slide_y,
        );

        hud::draw_hud(
            &self.state,
            &self.assets.hud,
            self.panel_top,
            &mut self.frame,
        );
        hud::draw_weapon_pod(
            firing,
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
                KeyEvent::Char(c) => match c.to_ascii_lowercase() {
                    'a' => self.nudge_overlay(-1, 0),
                    'd' => self.nudge_overlay(1, 0),
                    'w' => self.nudge_overlay(0, -1),
                    's' => self.nudge_overlay(0, 1),
                    _ => {}
                },
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
    fn wasd_nudges_the_overlay() {
        let mut scene = test_scene();
        let (x, y) = (scene.overlay_x, scene.overlay_offset_y);

        scene.update(Duration::ZERO, &[KeyEvent::Char('d'), KeyEvent::Char('s')]);
        assert_eq!((scene.overlay_x, scene.overlay_offset_y), (x + 1, y + 1));

        scene.update(Duration::ZERO, &[KeyEvent::Char('a'), KeyEvent::Char('w')]);
        assert_eq!((scene.overlay_x, scene.overlay_offset_y), (x, y));
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

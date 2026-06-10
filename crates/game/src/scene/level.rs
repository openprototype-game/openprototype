//! A developer scene for live-testing the in-game level render.
//!
//! Not part of the normal front-end flow: the `--scene level` flag boots
//! straight into it. It scrolls the level's parallax background and composites
//! the HUD panel and the animated weapon pod on top, all into one 320x160 frame,
//! so the scroll, panel geometry, and the pod's open/settle animation can be
//! checked against footage.
//!
//! All four weapons start fully charged. Enter cycles the selected weapon
//! (replaying the pod and overlay animations), Up/Down pan the camera (the
//! 160-tall background over the ~128-row window), WASD nudge the overlay, Esc quits.

use std::rc::Rc;
use std::time::Duration;

use prototype_formats::Dimensions;

use crate::assets::LevelAssets;
use crate::background::BackgroundScroll;
use crate::hud::{self, POD_SETTLED_FRAME};
use crate::level::prng::{EngineRng, clock_seed};
use crate::scene::{Scene, SceneOutput, Transition};
use crate::scenery::SceneryScroll;
use crate::stars::StarField;
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::input::KeyEvent;
use openprototype_core::{
    ActiveWeapon, GameState, Lives, PerWeapon, SmartBombs, Weapon, WeaponLevel,
};

/// The level's frame: hand-programmed Mode X 320x160 (480 scanlines, each row
/// tripled to give 160 logical rows), shown on a 4:3 CRT so pixels are 1.5x
/// taller than wide. The compositor fits this 320x160 buffer into 4:3, which
/// reproduces that stretch. Playfield is rows 0..128, the panel rows 128..160.
const SCREEN: Dimensions = Dimensions {
    width: 320,
    height: 160,
};

/// The game's logic tick. The original is vsync-locked: it calibrates the PIT
/// against the VGA vertical retrace (vaddr `0x9350`), so its tick is the display
/// refresh, ~60Hz for the 480-line mode. The parallax scroll and the pod
/// animation both advance on this tick.
const TICK: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// Ticks the weapon pod holds on each open/settle frame.
///
/// TODO: 4 ticks (~67ms) is an unverified placeholder, picked so the animation
/// is visible. The faithful divider on the anim counter `cs:0x2699` is not yet
/// traced.
const POD_FRAME_TICKS: u32 = 4;

/// How far the camera can pan: the 160-tall background minus the ~128-row playfield
/// window. Matches the original's vertical-scroll counter `cs:0x266e` (0..0x20).
const CAMERA_MAX: i32 = 32;

/// The overlay's x. Pinned against footage; it lands on the weapon pod's column
/// (the pod draws at screen x 252, `di` 0x3f), so the cut-off weapon top sits
/// directly above its pod. Still nudgeable with A/D.
const OVERLAY_X: i32 = 251;

/// The overlay's top, as rows above [`hud::PANEL_TOP`]. Pinned: `-7` is the
/// overlay's own height, so its bottom edge meets the panel's top row and the
/// cut-off top extends up from there. Still nudgeable with W/S.
const OVERLAY_OFFSET_Y: i32 = -7;

pub struct LevelScene {
    assets: Rc<LevelAssets>,
    state: GameState,
    frame: Framebuffer,
    /// Per-strip scroll positions for the level's parallax background.
    background_scroll: BackgroundScroll,
    /// Per-layer scroll positions for the level's scenery, advanced each tick.
    scenery_scroll: SceneryScroll,
    /// The level's star field, seeded from the wall clock like the original.
    stars: StarField,
    /// Vertical camera, `0..=CAMERA_MAX`: which background row sits at the top of the
    /// playfield. Nudged with Up/Down.
    camera_y: i32,
    /// The overlay's screen x, nudged with A/D.
    overlay_x: i32,
    /// The overlay's top relative to [`hud::PANEL_TOP`], nudged with W/S.
    overlay_offset_y: i32,
    /// The pod's current animation frame, `0` (hidden) up to [`POD_SETTLED_FRAME`].
    pod_frame: usize,
    /// Ticks accumulated toward the next pod frame.
    pod_ticks: u32,
    /// Real time accumulated toward the next logic tick.
    tick_elapsed: Duration,
}

impl LevelScene {
    pub fn new(assets: Rc<LevelAssets>) -> Self {
        let state = GameState {
            score: 0,
            lives: Lives::new(3),
            smart_bombs: SmartBombs::new(3),
            weapons: PerWeapon::splat(WeaponLevel::new(WeaponLevel::MAX)),
            selected: Weapon::Multishot,
            invincible_ticks: 0,
        };

        eprintln!(
            "level scene: Enter cycles weapon, Up/Down pan the camera, \
             WASD nudge the overlay, Esc quits"
        );

        let frame = Framebuffer::new(SCREEN, assets.hud.palette.clone());
        let background_scroll = assets.background.scroll();
        let scenery_scroll = assets.scenery.scroll();
        let stars = StarField::new(assets.stars, &mut EngineRng::new(clock_seed()));
        let mut scene = Self {
            assets,
            state,
            frame,
            background_scroll,
            scenery_scroll,
            stars,
            camera_y: 0,
            overlay_x: OVERLAY_X,
            overlay_offset_y: OVERLAY_OFFSET_Y,
            pod_frame: POD_SETTLED_FRAME,
            pod_ticks: 0,
            tick_elapsed: Duration::ZERO,
        };
        scene.render();

        scene
    }

    /// Cycle to the next weapon and replay the pod's open/settle animation.
    fn cycle_weapon(&mut self) {
        self.state.cycle_weapon();
        self.pod_frame = 0;
        self.pod_ticks = 0;
    }

    /// Pan the camera by `delta` rows, clamped to `0..=CAMERA_MAX`.
    fn nudge_camera(&mut self, delta: i32) {
        self.camera_y = (self.camera_y + delta).clamp(0, CAMERA_MAX);
        eprintln!("camera_y = {}", self.camera_y);
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

    /// Advance the parallax scroll and the pod animation by `ticks`.
    fn advance(&mut self, ticks: u32) {
        self.assets
            .background
            .advance(&mut self.background_scroll, ticks);
        self.assets.scenery.advance(&mut self.scenery_scroll, ticks);
        self.stars.advance(ticks);
        self.pod_ticks += ticks;

        while self.pod_frame < POD_SETTLED_FRAME && self.pod_ticks >= POD_FRAME_TICKS {
            self.pod_ticks -= POD_FRAME_TICKS;
            self.pod_frame += 1;
        }
    }

    /// Composite the parallax background, the weapon overlay, the HUD, and the pod.
    ///
    /// The overlay is a playfield sprite, drawn before the panel so the opaque
    /// `PANEL.RAW` masks its lower rows. While the pod opens its slide keeps it
    /// at the panel's top edge (hidden behind the panel); it only clears the
    /// panel once it snaps up to its settled row. The original gates the
    /// playfield sprite blitter against the HUD band for the same effect.
    fn render(&mut self) {
        let active = self.state.active_weapon();

        self.assets.background.render(
            &self.background_scroll,
            &mut self.frame,
            self.camera_y,
            hud::PANEL_TOP,
        );

        self.stars
            .render(&mut self.frame, self.camera_y, hud::PANEL_TOP);

        self.assets.scenery.render(
            &self.scenery_scroll,
            &self.assets.catalog,
            &mut self.frame,
            self.camera_y,
        );

        // The chaingun has no weapon-top overlay; only a selected weapon draws one.
        if let ActiveWeapon::Selected(weapon) = active {
            let overlay = self.assets.overlays.get(weapon);
            let slide = self.assets.overlay_slide.get(weapon);
            let (slide_x, slide_y) = slide[self.pod_frame.min(slide.len() - 1)];
            self.frame.blit_transparent(
                &overlay.pixels,
                overlay.size,
                self.overlay_x + slide_x,
                hud::PANEL_TOP + self.overlay_offset_y + slide_y,
            );
        }

        hud::draw_hud(
            &self.state,
            &self.assets.hud,
            hud::PANEL_TOP,
            &mut self.frame,
        );
        hud::draw_weapon_pod(
            active,
            self.pod_frame,
            &self.assets.hud,
            hud::PANEL_TOP,
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
                KeyEvent::Up => self.nudge_camera(-1),
                KeyEvent::Down => self.nudge_camera(1),
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

        self.tick_elapsed += dt;
        let mut ticks = 0;
        while self.tick_elapsed >= TICK {
            self.tick_elapsed -= TICK;
            ticks += 1;
        }
        self.advance(ticks);
        self.render();

        output
    }

    fn framebuffer(&self) -> &Framebuffer {
        &self.frame
    }

    fn is_animating(&self) -> bool {
        // The background scrolls continuously, so the scene always needs redrawing.
        true
    }

    fn frame_interval(&self) -> Duration {
        // The level runs the 480-line Mode X at ~60Hz, not the front-end's ~70Hz.
        // Driving frames at this rate makes the platform's fixed `dt` exactly one
        // [`TICK`], so the scroll advances one tick per frame with no beating.
        TICK
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::test_level_assets;

    fn test_scene() -> LevelScene {
        LevelScene::new(Rc::new(test_level_assets()))
    }

    #[test]
    fn starts_with_all_weapons_charged_and_the_pod_settled() {
        let scene = test_scene();

        for weapon in Weapon::ALL {
            assert_eq!(scene.state.level(weapon).get(), WeaponLevel::MAX);
        }

        assert_eq!(scene.pod_frame, POD_SETTLED_FRAME);
        assert_eq!(scene.camera_y, 0);
    }

    #[test]
    fn enter_cycles_the_weapon_and_restarts_the_pod_animation() {
        let mut scene = test_scene();
        assert_eq!(
            scene.state.active_weapon(),
            ActiveWeapon::Selected(Weapon::Multishot)
        );

        scene.update(Duration::ZERO, &[KeyEvent::Enter]);

        assert_eq!(
            scene.state.active_weapon(),
            ActiveWeapon::Selected(Weapon::Burning)
        );
        assert_eq!(scene.pod_frame, 0);
    }

    #[test]
    fn the_pod_animation_advances_to_settled_then_stops() {
        let mut scene = test_scene();
        scene.update(Duration::ZERO, &[KeyEvent::Enter]);
        assert_eq!(scene.pod_frame, 0);

        // Enough ticks to carry frame 0 up to the settled frame and then hold.
        let ticks = POD_FRAME_TICKS * (POD_SETTLED_FRAME as u32 + 1);
        scene.update(TICK * ticks, &[]);

        assert_eq!(scene.pod_frame, POD_SETTLED_FRAME);
    }

    #[test]
    fn up_and_down_pan_the_camera_and_clamp_to_the_range() {
        let mut scene = test_scene();
        assert_eq!(scene.camera_y, 0);

        scene.update(Duration::ZERO, &[KeyEvent::Down]);
        assert_eq!(scene.camera_y, 1);

        // Up past the top clamps at 0.
        scene.update(Duration::ZERO, &[KeyEvent::Up, KeyEvent::Up]);
        assert_eq!(scene.camera_y, 0);

        // Down past the bottom clamps at CAMERA_MAX.
        for _ in 0..CAMERA_MAX + 5 {
            scene.update(Duration::ZERO, &[KeyEvent::Down]);
        }
        assert_eq!(scene.camera_y, CAMERA_MAX);
    }

    #[test]
    fn one_tick_of_real_time_advances_the_scroll_by_one() {
        let mut scene = test_scene();
        scene.update(TICK, &[]);
        // Strip 0 (rate 16 = 1px) moved one whole pixel after one tick.
        assert_eq!(scene.background_scroll.pixel_column(0), 1);
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

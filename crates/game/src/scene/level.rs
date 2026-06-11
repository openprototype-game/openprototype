//! A developer scene for live-testing the in-game level render.
//!
//! Not part of the normal front-end flow: the `--scene level` flag boots
//! straight into it. It scrolls the level's parallax background and composites
//! the HUD panel and the animated weapon pod on top, all into one 320x160 frame,
//! so the scroll, panel geometry, and the pod's open/settle animation can be
//! checked against footage.
//!
//! All four weapons start fully charged. The arrow keys fly the ship (which
//! drags the vertical camera, like the original), Shift cycles the selected
//! weapon (the original's switch key, replaying the pod and overlay
//! animations), WASD nudge the overlay, `[`/`]` adjust the selected weapon's
//! charge level (dev keys, for testing the per-level fire modes), Esc quits.

use std::rc::Rc;
use std::time::Duration;

use prototype_formats::Dimensions;

use crate::assets::LevelAssets;
use crate::background::BackgroundScroll;
use crate::combat::{self, CombatEvents};
use crate::hud::{self, POD_SETTLED_FRAME};
use crate::level::prng::{EngineRng, clock_seed};
use crate::playfield;
use crate::scene::{Scene, SceneId, SceneOutput, Transition};
use crate::scenery::SceneryScroll;
use crate::sfx::Sfx;
use crate::ship::{HeldKeys, Ship};
use crate::shots::Weapons;
use crate::spawns::Spawns;
use crate::stars::StarField;
use openprototype_core::audio::AudioCommand;
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::game_state::{HitOutcome, Severity};
use openprototype_core::input::{Key, KeyEvent};
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

/// The overlay's x. Pinned against footage; it lands on the weapon pod's column
/// (the pod draws at screen x 252, `di` 0x3f), so the cut-off weapon top sits
/// directly above its pod. Still nudgeable with A/D.
const OVERLAY_X: i32 = 251;

/// The overlay's top, as rows above [`playfield::PANEL_TOP`]. Pinned: `-7` is the
/// overlay's own height, so its bottom edge meets the panel's top row and the
/// cut-off top extends up from there. Still nudgeable with W/S.
const OVERLAY_OFFSET_Y: i32 = -7;

/// The ship death sequence's length: 23 explosion frames, 4 ticks each
/// (`cs:0x8f5f` stepping by 8 to `0xb8` on a 4-tick divider, file `0xb354`).
const DEATH_TICKS: u32 = 92;

/// Where the GET READY text draws: the original blits to VRAM offset `0xfb8`
/// on the frozen page = row 50, byte 24 of the 80-byte Mode X row = x 96.
const GET_READY_POS: (i32, i32) = (96, 50);

/// The GET READY string as the WAD stores it (`cs:0x6e7`, `$`-terminated).
const GET_READY_TEXT: &str = "GET READY..";

/// The level's run state (the original's freeze and dying globals,
/// `cs:0x29f7` and `cs:0x46b2`).
enum Flow {
    /// Frozen on the last composed frame with GET READY overlaid, waiting for
    /// fire released-then-pressed (level start and after each death).
    GetReady { fire_released: bool },
    Running,
    /// The ship's death explosion is playing; the world runs on with input,
    /// body contact and the ship hit tests gated off. The life is deducted
    /// when the sequence ends (the original's respawn-time decrement).
    Dying { ticks_left: u32 },
    /// Out of lives: hand back to the front-end.
    GameOver,
}

pub struct LevelScene {
    assets: Rc<LevelAssets>,
    state: GameState,
    frame: Framebuffer,
    /// Per-strip scroll positions for the level's parallax background.
    background_scroll: BackgroundScroll,
    /// Per-layer scroll positions for the level's scenery, advanced each tick.
    scenery_scroll: SceneryScroll,
    /// The enemy/pickup spawn schedule and live entities, when the level's
    /// spawn-position table is known.
    spawns: Option<Spawns>,
    /// The smart bomb's delayed-damage countdown (`cs:0x2745`): armed at 15
    /// on use, the field-wide damage lands when it reaches 1.
    bomb_countdown: u8,
    /// The level's star field, seeded from the wall clock like the original.
    stars: StarField,
    /// The player ship, flown with the arrow keys.
    ship: Ship,
    /// The player's fire state: cooldown, live shots, muzzle flash.
    weapons: Weapons,
    /// The sound-effect trigger state (the plasma hum's loop flag).
    sfx: Sfx,
    /// Whether the level's music track has been started (the original's
    /// pending-start flag, baked set, consumed once at level begin).
    music_started: bool,
    /// Ticks until the music restarts (the original's `cs:[0x69ce]` TOC-length
    /// countdown in the timer ISR; the track loops when it underflows).
    music_countdown: i64,
    /// Flight keys currently held, maintained from the key transitions.
    held: HeldKeys,
    /// Whether the fire key (Ctrl) is held.
    fire_held: bool,
    /// Vertical camera, `camera_min..=32`: which background row sits at the
    /// top of the playfield. The ship's flight drags it (see [`Ship::update`]).
    camera_y: i32,
    /// The overlay's screen x, nudged with A/D.
    overlay_x: i32,
    /// The overlay's top relative to [`playfield::PANEL_TOP`], nudged with W/S.
    overlay_offset_y: i32,
    /// The pod's current animation frame, `0` (hidden) up to [`POD_SETTLED_FRAME`].
    pod_frame: usize,
    /// Ticks accumulated toward the next pod frame.
    pod_ticks: u32,
    /// Real time accumulated toward the next logic tick.
    tick_elapsed: Duration,
    /// Debug freeze: P stops every scroll so stills can be compared against
    /// the original. Starts frozen-off; the scene begins at scroll zero anyway.
    paused: bool,
    /// Dev fast-forward: F held runs 8 logic ticks per frame.
    turbo: bool,
    /// Ticks left of the level-end sequence: the original runs 460 more
    /// frames after the boss dies, and for the last 300 the ship flies off
    /// to the right with input locked ([`Ship::fly_out`]).
    level_end_countdown: Option<u32>,
    /// The freeze/dying state; the level starts frozen on GET READY.
    flow: Flow,
}

impl LevelScene {
    pub fn new(assets: Rc<LevelAssets>, skip_ticks: u32) -> Self {
        let state = new_game_state();

        eprintln!(
            "level scene: arrows fly the ship, Ctrl fires, Shift cycles weapon, \
             Space smart-bombs, F fast-forwards, WASD nudge the overlay, \
             [/] adjust its level, P pauses the scroll, Esc quits"
        );

        let frame = Framebuffer::new(SCREEN, assets.hud.palette.clone());
        let background_scroll = assets.background.scroll();
        let scenery_scroll = assets.scenery.scroll();
        let stars = StarField::new(assets.stars, &mut EngineRng::new(clock_seed()));
        // The original seeds the layout PRNG from the wall clock at level
        // init, so the scatter varies every play.
        let spawns = assets.spawn_rows.is_some().then(|| {
            Spawns::new(
                assets.spawns.records(&assets.wad, clock_seed()),
                assets.spawn_ai,
            )
        });
        let ship = Ship::new(assets.ship);
        let weapons = Weapons::new(assets.bob_wave.clone(), state.active_weapon());
        let camera_y = assets.camera_min;
        let mut scene = Self {
            assets,
            state,
            frame,
            background_scroll,
            scenery_scroll,
            spawns,
            bomb_countdown: 0,
            stars,
            ship,
            weapons,
            sfx: Sfx::new(),
            music_started: false,
            music_countdown: 0,
            held: HeldKeys::default(),
            fire_held: false,
            camera_y,
            overlay_x: OVERLAY_X,
            overlay_offset_y: OVERLAY_OFFSET_Y,
            pod_frame: POD_SETTLED_FRAME,
            pod_ticks: 0,
            tick_elapsed: Duration::ZERO,
            paused: false,
            turbo: false,
            level_end_countdown: None,
            flow: Flow::GetReady {
                fire_released: false,
            },
        };
        scene.fast_forward(skip_ticks);
        scene.render();

        scene
    }

    /// Dev fast-forward (`--skip`): pre-simulates `ticks` of the level with
    /// the ship parked and shielded, then leaves the respawn shield up so the
    /// player can orient. Sounds from the skipped span are dropped.
    ///
    /// Gate fights can't resolve with a parked ship, so the skip clears them:
    /// orbiters are auto-killed (their deaths release the gate through the
    /// regular reap) and a gating form-1 boss is dropped to its dying
    /// threshold so its retreat plays out during the skip. The final boss
    /// form's arrival ends the skip, so a large `--skip` lands at the last
    /// phase with the fight still ahead.
    fn fast_forward(&mut self, ticks: u32) {
        if ticks == 0 {
            return;
        }

        const BOSS_FORM_2: u16 = 23;

        self.flow = Flow::Running;
        let mut scratch = Vec::new();

        for _ in 0..ticks {
            if let Some(spawns) = &mut self.spawns {
                if spawns.entities.iter().any(|e| e.arg == BOSS_FORM_2) {
                    tracing::info!("skip stopped at the final boss form");
                    break;
                }

                if spawns.gate_holds() {
                    let mut orbiters = 0;

                    for entity in &mut spawns.entities {
                        if entity.health > 0 && (0x392e..=0x399c).contains(&entity.sprite) {
                            entity.health = 0;
                            orbiters += 1;
                        }
                    }

                    // No orbiters means a boss holds the gate: "defeat" it by
                    // dropping it to the dying threshold (the next AI step
                    // starts the death script).
                    if orbiters == 0 {
                        for entity in &mut spawns.entities {
                            if entity.health > 0x1388 && entity.kind >= 0x3ae8 {
                                tracing::info!("skip auto-defeated a gating boss form");
                                entity.health = 0x1388;
                            }
                        }
                    }
                }
            }

            self.state.invincible_ticks = self.state.invincible_ticks.max(2);
            self.advance(1, &mut scratch);
            scratch.clear();
        }

        self.state.invincible_ticks = 300;
        self.ship.arm_shield(300);
        // Land frozen on GET READY so the player starts on their own cue.
        self.flow = Flow::GetReady {
            fire_released: false,
        };
    }

    /// Cycle to the next weapon and replay the pod's open/settle animation.
    fn cycle_weapon(&mut self) {
        self.state.cycle_weapon();
        self.pod_frame = 0;
        self.pod_ticks = 0;
    }

    /// Report the selected weapon's charge after a dev-key adjustment.
    fn report_level(&self) {
        eprintln!(
            "{:?} level = {}",
            self.state.selected,
            self.state.level(self.state.selected).get()
        );
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

    /// Start the music and loop it on the track-length countdown, like the
    /// original's timer ISR.
    ///
    /// The start waits for the first GET READY dismissal: the original bakes
    /// a pending-start flag (`cs:0x736c`) that the first unfreeze consumes
    /// (file `0x9e65`), so the opening GET READY is silent and respawn
    /// freezes don't restart the track.
    fn advance_music(&mut self, ticks: u32, audio: &mut Vec<AudioCommand>) {
        if !self.music_started {
            if matches!(self.flow, Flow::GetReady { .. }) {
                return;
            }

            self.music_started = true;
            audio.push(AudioCommand::PlayTrack(self.assets.music.track));
            self.music_countdown = i64::from(self.assets.music.length_ticks);
        }

        for _ in 0..ticks {
            self.music_countdown -= 1;

            if self.music_countdown < 0 {
                audio.push(AudioCommand::PlayTrack(self.assets.music.track));
                self.music_countdown = i64::from(self.assets.music.length_ticks);
            }
        }
    }

    /// Advance the ship, the parallax scroll, the spawn clock, and the pod
    /// animation by `ticks`, collecting the fire pass's sound triggers into
    /// `audio`.
    fn advance(&mut self, ticks: u32, audio: &mut Vec<AudioCommand>) {
        for _ in 0..ticks {
            self.state.tick();
            self.run_combat(audio);

            // The death sequence: the world keeps running, the ship doesn't.
            // When the explosion finishes, the life comes off and the level
            // freezes into the respawn GET READY, or exits when that was the
            // last one (the original's respawn handler, file 0x9d84).
            if let Flow::Dying { ticks_left } = &mut self.flow {
                *ticks_left -= 1;

                if *ticks_left == 0 {
                    if self.state.lose_life() == HitOutcome::GameOver {
                        self.flow = Flow::GameOver;
                    } else {
                        self.ship = Ship::new(self.assets.ship);
                        self.flow = Flow::GetReady {
                            fire_released: false,
                        };
                    }

                    break;
                }
            }

            let running = matches!(self.flow, Flow::Running);

            if running {
                self.ship
                    .update(self.held, &mut self.camera_y, self.assets.camera_min);
            }

            let enemy_count = self
                .spawns
                .as_ref()
                .map_or(0, |spawns| spawns.entities.len());
            let sounds = self.weapons.update(
                self.fire_held && running,
                &self.state,
                self.ship.position(),
                self.ship.roll_frame(),
                &self.assets.barrel_offsets,
                enemy_count,
            );

            if sounds.switched {
                self.sfx.weapon_switched(&self.assets.sfx, audio);
            }

            if let Some(weapon) = sounds.fired {
                self.sfx.weapon_fired(weapon, &self.assets.sfx, audio);
            }

            if sounds.launched {
                self.sfx.plasma_launched(audio);
            }
        }

        // The boss/orbiter gate holds the parallax (the original's ISR skips
        // the scroll block while cs:0x269c is up).
        let gate_holds = self
            .spawns
            .as_ref()
            .is_some_and(|spawns| spawns.gate_holds());

        if !gate_holds {
            self.assets
                .background
                .advance(&mut self.background_scroll, ticks);
            self.assets.scenery.advance(&mut self.scenery_scroll, ticks);
        }
        self.stars.advance(ticks);
        self.pod_ticks += ticks;

        while self.pod_frame < POD_SETTLED_FRAME && self.pod_ticks >= POD_FRAME_TICKS {
            self.pod_ticks -= POD_FRAME_TICKS;
            self.pod_frame += 1;
        }
    }

    /// One tick of the spawn clock, enemy movement, and every combat pass.
    fn run_combat(&mut self, audio: &mut Vec<AudioCommand>) {
        let (Some(spawns), Some(rows)) = (&mut self.spawns, &self.assets.spawn_rows) else {
            return;
        };

        let wad = &self.assets.wad;
        let cs_base = self.assets.cs_base;

        spawns.tick(rows, wad, cs_base);

        let shots_before = spawns.shots.len();
        spawns.step_movement(wad, self.ship.position());

        if spawns.shots.len() > shots_before {
            self.sfx.enemy_fired(&self.assets.sfx, audio);
        }

        let mut events = CombatEvents::default();

        // The smart bomb's field-wide damage lands 14 ticks after use
        // (`cs:0x2745` reaching 1 sets the one-frame 600 across the board).
        if self.bomb_countdown > 0 {
            self.bomb_countdown -= 1;

            if self.bomb_countdown == 1 {
                self.bomb_countdown = 0;

                for entity in &mut spawns.entities {
                    entity.health -= 600;
                }

                combat::reap(spawns, wad, cs_base, &mut events);
            }
        }

        combat::player_shots(&mut self.weapons, spawns, wad, cs_base, &mut events);

        // The missiles steer between the hit test and their move (the
        // original runs file 0xc114 per movement sub-step in the shot pass).
        self.weapons
            .steer_missiles(&spawns.entities, wad, cs_base, &mut spawns.effects);

        // While the death explosion plays, the ship has no presence: the
        // original gates the ship hit test and body contact off the dying
        // flag (`cs:0x46b2`). Far-away rects disable both passes here.
        let rects = if matches!(self.flow, Flow::Dying { .. }) {
            [[i32::MAX, i32::MAX, i32::MAX, i32::MAX]; 3]
        } else {
            let (ship_x, ship_y) = self.ship.position();
            combat::ship_rects(wad, cs_base, self.ship.roll_frame(), ship_x, ship_y)
        };

        for _ in 0..combat::enemy_shots_vs_ship(spawns, wad, cs_base, &rects) {
            let outcome = self.state.take_hit(Severity::Bullet);
            events.ship = merge_outcome(events.ship, outcome);

            if outcome == HitOutcome::Absorbed {
                // A drain to zero loses the weapon mid-hold (the original's
                // immediate revert to the minigun, with its own sound).
                if self.state.level(self.state.selected).get() == 0 {
                    self.weapons.weapon_lost();
                    self.sfx.weapon_lost(&self.assets.sfx, audio);
                } else {
                    self.sfx.weapon_drained(&self.assets.sfx, audio);
                }
            }
        }

        combat::body_contact(spawns, &rects, &mut self.state, wad, cs_base, &mut events);

        self.state.add_score(events.score);

        if events.level_end && self.level_end_countdown.is_none() {
            tracing::info!(score = self.state.score, "level complete");
            self.level_end_countdown = Some(460);
        }

        if let Some(form2) = spawns.boss_explosion.take() {
            self.sfx.boss_explosion(form2, &self.assets.sfx, audio);
        }

        if spawns.pod_deployed {
            spawns.pod_deployed = false;
            self.sfx.pod_deployed(&self.assets.sfx, audio);
        }

        // Impact sounds first, then the death sounds: the original's frame
        // runs the collision pass (sparks) before the update loop (deaths),
        // so a kill's explosion replaces the impact on their shared channel.
        if events.chaingun_impact {
            self.sfx.chaingun_impact(&self.assets.sfx, audio);
        }

        if events.missile_impact {
            self.sfx.missile_impact(&self.assets.sfx, audio);
        }

        for kind in &events.kills {
            self.sfx.enemy_died(*kind, &self.assets.sfx, audio);
        }

        if events.orb_dropped {
            self.sfx.orb_dropped(&self.assets.sfx, audio);
        }

        if events.pickup {
            self.sfx.pickup_collected(&self.assets.sfx, audio);
        }

        if events.shield_pickup {
            self.ship.arm_shield(600);
        }

        if events.ram == Some(HitOutcome::Absorbed) {
            self.weapons.weapon_lost();
            self.sfx.weapon_lost(&self.assets.sfx, audio);
        }

        // A fatal hit starts the death explosion; the life loss and the
        // respawn (or the game-over exit) happen when the sequence finishes,
        // in `advance`.
        if events.ship == Some(HitOutcome::Died) {
            self.sfx.ship_died(&self.assets.sfx, audio);
            self.flow = Flow::Dying {
                ticks_left: DEATH_TICKS,
            };
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
            playfield::PANEL_TOP,
        );

        self.stars
            .render(&mut self.frame, self.camera_y, playfield::PANEL_TOP);

        self.assets.scenery.render_behind(
            &self.scenery_scroll,
            &self.assets.catalog,
            &mut self.frame,
            self.camera_y,
        );

        if let Some(spawns) = &mut self.spawns {
            spawns.render(
                &self.assets.wad,
                self.assets.cs_base,
                &self.assets.clip_catalog,
                &mut self.frame,
                self.camera_y,
            );
        }

        self.weapons
            .render(&self.assets.fire_sprites, &mut self.frame, self.camera_y);

        self.ship.render(
            &self.assets.ship_frames,
            &self.assets.shield_frames,
            &mut self.frame,
            self.camera_y,
        );

        // The death explosion draws over the ship at its position (the dying
        // branch of the ship draw, file 0xbafd).
        if let Flow::Dying { ticks_left, .. } = self.flow {
            let frame_index = ((DEATH_TICKS - ticks_left) / 4) as usize;

            if let Some(sprite) = self.assets.ship_explosion.get(frame_index) {
                let (x, y) = self.ship.position();
                self.frame.blit_transparent(
                    &sprite.pixels,
                    sprite.size,
                    playfield::LEFT + x,
                    y - self.camera_y,
                );
            }
        }

        self.weapons.render_flash(
            &self.assets.fire_sprites,
            &mut self.frame,
            self.ship.position(),
            self.ship.roll_frame(),
            &self.assets.barrel_offsets,
            self.camera_y,
        );

        self.assets.scenery.render_front(
            &self.scenery_scroll,
            &self.assets.catalog,
            &mut self.frame,
            self.camera_y,
        );

        self.mask_playfield_margins();

        // The chaingun has no weapon-top overlay; only a selected weapon draws one.
        if let ActiveWeapon::Selected(weapon) = active {
            let overlay = self.assets.overlays.get(weapon);
            let slide = self.assets.overlay_slide.get(weapon);
            let (slide_x, slide_y) = slide[self.pod_frame.min(slide.len() - 1)];
            self.frame.blit_transparent(
                &overlay.pixels,
                overlay.size,
                self.overlay_x + slide_x,
                playfield::PANEL_TOP + self.overlay_offset_y + slide_y,
            );
        }

        hud::draw_hud(
            &self.state,
            &self.assets.hud,
            playfield::PANEL_TOP,
            &mut self.frame,
        );
        hud::draw_weapon_pod(
            active,
            self.pod_frame,
            &self.assets.hud,
            playfield::PANEL_TOP,
            &mut self.frame,
        );

        if matches!(self.flow, Flow::GetReady { .. }) {
            self.dim_playfield();
            self.assets.font.draw_into(
                &mut self.frame.image,
                GET_READY_POS.0,
                GET_READY_POS.1,
                GET_READY_TEXT,
            );
        }
    }

    /// The GET READY freeze darkens the playfield but not the panel (file
    /// `0xe60f`): every playfield pixel is remapped through the level's
    /// third-brightness table before the text draws over it.
    fn dim_playfield(&mut self) {
        let playfield_pixels = (SCREEN.width * playfield::PANEL_TOP as u32) as usize;

        for pixel in &mut self.frame.image.pixels[..playfield_pixels] {
            *pixel = self.assets.dim_table[usize::from(*pixel)];
        }
    }

    /// Black out everything outside the playfield window, standing in for the
    /// original's compose-buffer blit: it copies only the window's 72 bytes
    /// per row to VGA (see [`playfield::LEFT`]), so the VGA side bars are
    /// never written and whatever the layers bled past the window is dropped.
    fn mask_playfield_margins(&mut self) {
        let width = self.frame.image.size.width as usize;
        let left = playfield::LEFT as usize;
        let right = (playfield::LEFT + playfield::WIDTH) as usize;

        for row in 0..playfield::PANEL_TOP as usize {
            let row_start = row * width;
            self.frame.image.pixels[row_start..row_start + left].fill(0);
            self.frame.image.pixels[row_start + right..row_start + width].fill(0);
        }
    }
}

impl Scene for LevelScene {
    fn update(&mut self, dt: Duration, input: &[KeyEvent]) -> SceneOutput {
        let mut output = SceneOutput::default();

        for event in input {
            match *event {
                KeyEvent::Pressed(key) => match key {
                    // The weapon switch and the smart bomb only respond in
                    // free flight (the original skips input while frozen or
                    // dying).
                    Key::Shift => {
                        if matches!(self.flow, Flow::Running) {
                            self.cycle_weapon();
                        }
                    }
                    Key::Char('f') => self.turbo = true,
                    Key::Char(' ') => {
                        if matches!(self.flow, Flow::Running)
                            && self.bomb_countdown == 0
                            && self.state.use_smart_bomb()
                        {
                            self.bomb_countdown = 15;
                            self.weapons
                                .smart_bomb(self.ship.position(), &self.assets.bomb_wave);
                        }
                    }
                    Key::Up => self.held.up = true,
                    Key::Down => self.held.down = true,
                    Key::Left => self.held.left = true,
                    Key::Right => self.held.right = true,
                    Key::Esc => output.transition = Some(Transition::Quit),
                    Key::Ctrl => self.fire_held = true,
                    Key::Enter | Key::Backspace => {}
                    Key::Char(c) => match c.to_ascii_lowercase() {
                        'a' => self.nudge_overlay(-1, 0),
                        'd' => self.nudge_overlay(1, 0),
                        'w' => self.nudge_overlay(0, -1),
                        's' => self.nudge_overlay(0, 1),
                        'p' => {
                            self.paused = !self.paused;
                            eprintln!("scroll {}", if self.paused { "paused" } else { "running" });
                        }
                        // Dev: adjust the selected weapon's charge level, to
                        // test the per-level fire modes.
                        ']' => {
                            self.state.level_up();
                            self.report_level();
                        }
                        '[' => {
                            let level = self.state.weapons.get_mut(self.state.selected);
                            *level = level.saturating_sub(1);
                            self.report_level();
                        }
                        _ => {}
                    },
                },
                KeyEvent::Released(key) => match key {
                    Key::Up => self.held.up = false,
                    Key::Down => self.held.down = false,
                    Key::Left => self.held.left = false,
                    Key::Right => self.held.right = false,
                    Key::Ctrl => self.fire_held = false,
                    Key::Char('f') => self.turbo = false,
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

        // Dev fast-forward: F held runs the level at 8x.
        if self.turbo {
            ticks *= 8;
        }

        // The GET READY freeze waits for fire released, then pressed (the
        // respawn handler's two key loops at file 0x9d84).
        if let Flow::GetReady { fire_released } = &mut self.flow {
            if !self.fire_held {
                *fire_released = true;
            } else if *fire_released {
                self.flow = Flow::Running;
            }
        }

        // The music runs off the timer ISR in the original, so neither the
        // dev pause nor the GET READY freeze stops its loop countdown.
        self.advance_music(ticks, &mut output.audio);

        let frozen = matches!(self.flow, Flow::GetReady { .. } | Flow::GameOver);

        if !self.paused && !frozen {
            self.advance(ticks, &mut output.audio);
        }

        // Out of lives: the original exits to the front-end with status 5,
        // which runs the game-over sequence and the high-score check.
        if matches!(self.flow, Flow::GameOver) {
            output.transition = Some(Transition::To(SceneId::GameOver {
                score: self.state.score,
            }));
        }

        // The level-end sequence (file 0xf866): the boss died, the game runs
        // on for 460 frames, and for the last 300 the ship's controls lock
        // and it flies off the right edge. Then the level hands back (the
        // original exits to the front-end; the next-level flow is not built
        // yet).
        if let Some(countdown) = &mut self.level_end_countdown {
            *countdown = countdown.saturating_sub(ticks);

            if *countdown < 300 {
                self.ship.fly_out();
            }

            if *countdown == 0 {
                output.transition = Some(Transition::To(SceneId::MainMenu));
            }
        }

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

/// The fresh-game player state (the original's new-game init is untraced;
/// these are the dev scene's starting values).
fn new_game_state() -> GameState {
    GameState {
        score: 0,
        lives: Lives::new(3),
        smart_bombs: SmartBombs::new(3),
        weapons: PerWeapon::splat(WeaponLevel::new(WeaponLevel::MAX)),
        selected: Weapon::Multishot,
        // The level-start GET READY arms the same 300-tick shield as a
        // respawn (the original's `0x266a = 0x12c` init).
        invincible_ticks: 300,
    }
}

/// Keeps the worse of two ship outcomes across the tick's passes.
fn merge_outcome(current: Option<HitOutcome>, new: HitOutcome) -> Option<HitOutcome> {
    match (current, new) {
        (Some(HitOutcome::Died), _) | (_, HitOutcome::Died) => Some(HitOutcome::Died),
        (current, new) => current.or(Some(new)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::test_level_assets;

    /// A scene past the GET READY freeze, so ticks advance the world.
    fn test_scene() -> LevelScene {
        let mut scene = LevelScene::new(Rc::new(test_level_assets()), 0);
        scene.flow = Flow::Running;

        scene
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
    fn shift_cycles_the_weapon_and_restarts_the_pod_animation() {
        let mut scene = test_scene();
        assert_eq!(
            scene.state.active_weapon(),
            ActiveWeapon::Selected(Weapon::Multishot)
        );

        scene.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Shift)]);

        assert_eq!(
            scene.state.active_weapon(),
            ActiveWeapon::Selected(Weapon::Burning)
        );
        assert_eq!(scene.pod_frame, 0);
    }

    #[test]
    fn the_pod_animation_advances_to_settled_then_stops() {
        let mut scene = test_scene();
        scene.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Shift)]);
        assert_eq!(scene.pod_frame, 0);

        // Enough ticks to carry frame 0 up to the settled frame and then hold.
        let ticks = POD_FRAME_TICKS * (POD_SETTLED_FRAME as u32 + 1);
        scene.update(TICK * ticks, &[]);

        assert_eq!(scene.pod_frame, POD_SETTLED_FRAME);
    }

    #[test]
    fn flying_down_drags_the_camera_to_its_stop_and_release_holds_the_ship() {
        let mut scene = test_scene();
        assert_eq!(scene.camera_y, 0);

        // Hold Down through the spawn ramp and far past the pan threshold:
        // the ship's flight drags the camera all the way to its lower stop.
        scene.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Down)]);
        scene.update(TICK * 200, &[]);
        assert_eq!(scene.camera_y, 32);

        // Releasing the key stops the ship where it is.
        scene.update(Duration::ZERO, &[KeyEvent::Released(Key::Down)]);
        let position = scene.ship.position();
        scene.update(TICK * 10, &[]);
        assert_eq!(scene.ship.position(), position);
    }

    #[test]
    fn one_tick_of_real_time_advances_the_scroll_by_one() {
        let mut scene = test_scene();
        let start = scene.background_scroll.pixel_column(0);
        scene.update(TICK, &[]);
        // Strip 0 (rate 16 = 1px) moved one whole pixel after one tick.
        assert_eq!(scene.background_scroll.pixel_column(0), start + 1);
    }

    #[test]
    fn wasd_nudges_the_overlay() {
        let mut scene = test_scene();
        let (x, y) = (scene.overlay_x, scene.overlay_offset_y);

        scene.update(
            Duration::ZERO,
            &[
                KeyEvent::Pressed(Key::Char('d')),
                KeyEvent::Pressed(Key::Char('s')),
            ],
        );
        assert_eq!((scene.overlay_x, scene.overlay_offset_y), (x + 1, y + 1));

        scene.update(
            Duration::ZERO,
            &[
                KeyEvent::Pressed(Key::Char('a')),
                KeyEvent::Pressed(Key::Char('w')),
            ],
        );
        assert_eq!((scene.overlay_x, scene.overlay_offset_y), (x, y));
    }

    #[test]
    fn the_music_starts_on_the_first_frame_and_loops_on_the_track_length() {
        let mut scene = test_scene();
        let track = scene.assets.music.track;

        // The first frame starts the track, even without an elapsed tick.
        let output = scene.update(Duration::ZERO, &[]);
        assert_eq!(output.audio, vec![AudioCommand::PlayTrack(track)]);

        // Nothing replays while the countdown runs (the test assets' track
        // is 10 ticks long).
        let output = scene.update(TICK * 10, &[]);
        assert_eq!(output.audio, vec![]);

        // The countdown underflows one tick past the length: restart.
        let output = scene.update(TICK, &[]);
        assert_eq!(output.audio, vec![AudioCommand::PlayTrack(track)]);
    }

    #[test]
    fn esc_quits() {
        let mut scene = test_scene();

        assert_eq!(
            scene
                .update(Duration::ZERO, &[KeyEvent::Pressed(Key::Esc)])
                .transition,
            Some(Transition::Quit)
        );
    }
}

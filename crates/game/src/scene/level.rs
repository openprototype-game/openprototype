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

use prototype_formats::{Dimensions, Rgb};

use crate::assets::LevelAssets;
use crate::background::BackgroundScroll;
use crate::combat::{self, CombatEvents};
use crate::hud::{self, POD_SETTLED_FRAME};
use crate::ingame_menu::{InGameMenu, MenuRequest};
use crate::level::prng::{EngineRng, clock_seed};
use crate::levels::Level;
use crate::playfield;
use crate::scene::{Scene, SceneId, SceneOutput, Transition};
use crate::scenery::SceneryScroll;
use crate::sfx::Sfx;
use crate::ship::{self, HeldKeys, Ship};
use crate::shots::Weapons;
use crate::spawns::{PlayerInput, Spawns};
use crate::stars::StarField;
use openprototype_core::audio::AudioCommand;
use openprototype_core::framebuffer::Framebuffer;
use openprototype_core::game_state::{Handoff, HitOutcome, Severity};
use openprototype_core::input::{Key, KeyEvent};
use openprototype_core::{ActiveWeapon, GameState, Weapon};

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

/// Ticks the weapon pod holds on each open/settle frame: the animator steps
/// its phase on every 6th tick (the divider on `cs:0x269b`, L1 file `0xab4a`,
/// byte-identical in all seven WADs).
const POD_FRAME_TICKS: u32 = 6;

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

/// The freeze pulse's palette entries and their full-brightness colors
/// (6-bit DAC; RGB tables at L2 file `0x7832`, byte-identical in every
/// WAD). In FONT.RAW these four indices appear only in the `>` glyph — the
/// pause menu's cursor, the one thing on a frozen screen that visibly
/// pulses. The level palettes bake placeholders at `0xd0..0xd2` (magenta
/// on the races); this pulse is their only writer.
const PULSE_ENTRIES: [(usize, [u8; 3]); 4] = [
    (0xd0, [0x3f, 0x22, 0x08]),
    (0xd1, [0x3e, 0x04, 0x00]),
    (0xd2, [0x20, 0x02, 0x00]),
    (0xff, [0x3f, 0x3f, 0x3f]),
];

/// The pulse's shared dark endpoint (every entry lerps toward the same
/// near-black blue).
const PULSE_SOURCE: [u8; 3] = [0x00, 0x00, 0x20];

/// The pulse level's bounce bounds and tick step (L2 stepper file `0x8f98`:
/// `v` ping-pongs `0x10..=0x40` by 2, endpoints held one tick each, period
/// 48 ticks).
const PULSE_MIN: i32 = 0x10;
const PULSE_MAX: i32 = 0x40;
const PULSE_STEP: i32 = 2;

/// The pulse level the WAD bakes at level entry, headed down.
const PULSE_START: i32 = 0x30;

/// The level's run state (the original's freeze and dying globals,
/// `cs:0x29f7` and `cs:0x46b2`).
enum Flow {
    /// Frozen on the last composed frame with GET READY overlaid, waiting for
    /// fire released-then-pressed (level start and after each death).
    GetReady {
        fire_released: bool,
    },
    Running,
    /// The ship's death explosion is playing; the world runs on with input,
    /// body contact and the ship hit tests gated off. The life is deducted
    /// when the sequence ends (the original's respawn-time decrement).
    Dying {
        ticks_left: u32,
    },
    /// Out of lives: hand back to the front-end.
    GameOver,
}

pub struct LevelScene {
    assets: Rc<LevelAssets>,
    /// Which level this is, for the end-of-level chain to the next.
    level: Level,
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
    /// Whether the launch-time CD stop has been issued (once, on the first
    /// update).
    music_stopped: bool,
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
    /// The Esc menu, freezing the level while open.
    menu: Option<InGameMenu>,
    /// The freeze pulse's level and direction (`cs:0x6ea6`/`0x6ea7` in
    /// L2). Never reset: the phase persists across freezes within the level.
    pulse_value: i32,
    pulse_rising: bool,
}

impl LevelScene {
    pub fn new(assets: Rc<LevelAssets>, level: Level, handoff: Handoff, skip_ticks: u32) -> Self {
        // The level start runs the original's spawn handler on its respawn
        // path (the handoff flag `cs:0xb12b` bakes 0 and only an `f:message`
        // mode byte changes it), so on top of the carried payload's entry
        // math it arms the same invincibility a death respawn does.
        let mut state = GameState::enter_level(handoff);
        state.invincible_ticks = assets.combat.respawn_invincibility;

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
                assets.combat,
            )
        });
        let mut ship = Ship::new(assets.ship);
        ship.arm_shield(i32::from(assets.combat.respawn_invincibility));
        let weapons = Weapons::new(assets.bob_wave.clone(), state.active_weapon());
        let camera_y = assets.camera_min;
        // The lose-on-entry edge: a one-life carry without the 10,000-point
        // refund nets zero lives, and the original exits to game over before
        // the level even shows.
        let flow = if state.lives.get() == 0 {
            Flow::GameOver
        } else {
            Flow::GetReady {
                fire_released: false,
            }
        };
        let mut scene = Self {
            assets,
            level,
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
            music_stopped: false,
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
            flow,
            menu: None,
            pulse_value: PULSE_START,
            pulse_rising: false,
        };

        if !matches!(scene.flow, Flow::GameOver) {
            scene.fast_forward(skip_ticks);
        }

        scene.render();

        scene
    }

    /// Rebuild a level mid-action from a savegame snapshot.
    ///
    /// Mirrors the original's load path: the spawn handler's entry
    /// decrement and spawn shield are skipped (the handoff flag is nonzero
    /// on a load), the player state and live objects restore verbatim, and
    /// play resumes frozen on GET READY. The shield bubble tracks whatever
    /// invincibility the snapshot carried.
    ///
    /// The scroll accumulators restore through the level's saved slot
    /// order ([`crate::savegame::scroll_layout`]): the scenery layers and
    /// the SP background strips one-to-one. The derived slots (the races'
    /// star planes, the dead layer slots) have no port state to restore —
    /// the original cannot restore the star scatter either.
    pub fn from_save(assets: Rc<LevelAssets>, save: crate::savegame::SaveGame) -> Self {
        let mut scene = Self::new(assets, save.level, save.handoff(), 0);

        scene.state = save.state;
        scene
            .ship
            .restore(save.ship_x, save.ship_y, save.ship_ramp, save.ship_roll);
        scene
            .ship
            .arm_shield(i32::from(scene.state.invincible_ticks));
        scene.camera_y = i32::from(save.speed_level);
        scene.spawns = Some(Spawns::from_save(
            save.records,
            save.cursor,
            save.entities,
            save.enemy_shots,
            save.effects,
            save.orb_drop_countdown,
            save.level_end,
            scene.assets.spawn_ai,
            scene.assets.combat,
            &scene.assets.wad,
            scene.assets.cs_base,
        ));
        let layout = crate::savegame::scroll_layout(save.level);

        for (index, slot) in layout.leading.iter().enumerate() {
            if let (crate::savegame::ScrollSlot::Scenery(layer), Some(&accum)) =
                (slot, save.scroll_accums.get(index))
            {
                scene
                    .assets
                    .scenery
                    .restore_offset(&mut scene.scenery_scroll, *layer, accum);
            }
        }

        for strip in 0..layout.strips {
            if let Some(&accum) = save.scroll_accums.get(layout.leading.len() + strip) {
                scene.background_scroll.restore_offset(strip, accum);
            }
        }

        scene.render();

        scene
    }

    /// Snapshot the running level as a savegame, the inverse of
    /// [`LevelScene::from_save`].
    ///
    /// The schedule stores the head record's delay decayed to the live
    /// countdown, the way the original's ISR-mutated table reads when the
    /// writer dumps it. The accumulators come from the level's saved slot
    /// order ([`crate::savegame::scroll_layout`]): scenery layers and SP
    /// strips from the live scroll state, the derived slots (star planes,
    /// dead layer slots) as rate times the elapsed-tick estimate, which
    /// matches the original until the first strip's wrap (their values are
    /// not restorable noise either way).
    pub fn to_save(&self) -> crate::savegame::SaveGame {
        let (records, cursor, orb_drop_countdown, level_end, entities, enemy_shots, effects) =
            match &self.spawns {
                Some(spawns) => {
                    let (records, cursor) = spawns.save_schedule();

                    (
                        records,
                        cursor,
                        spawns.orb_drop_countdown(),
                        spawns.level_end,
                        spawns.entities.clone(),
                        spawns.shots.clone(),
                        spawns.effects.clone(),
                    )
                }
                None => (Vec::new(), 0, 0, false, Vec::new(), Vec::new(), Vec::new()),
            };

        let layout = crate::savegame::scroll_layout(self.level);
        let consts = crate::savegame::scroll_consts(self.level);
        let elapsed_ticks = self.background_scroll.offset(0) / consts[layout.leading.len()].max(1);

        let slot_value = |index: usize, slot: &crate::savegame::ScrollSlot| match slot {
            crate::savegame::ScrollSlot::Scenery(layer) => self.scenery_scroll.offset(*layer),
            crate::savegame::ScrollSlot::Derived => elapsed_ticks * consts[index],
        };

        let mut scroll_accums = Vec::with_capacity(consts.len());

        for (index, slot) in layout.leading.iter().enumerate() {
            scroll_accums.push(slot_value(index, slot));
        }

        for strip in 0..layout.strips {
            scroll_accums.push(self.background_scroll.offset(strip));
        }

        for (offset, slot) in layout.trailing.iter().enumerate() {
            scroll_accums.push(slot_value(
                layout.leading.len() + layout.strips + offset,
                slot,
            ));
        }

        let (ship_x, ship_y) = self.ship.position();

        crate::savegame::SaveGame {
            level: self.level,
            state: self.state.clone(),
            records,
            cursor,
            orb_drop_countdown,
            level_end,
            entities,
            enemy_shots,
            effects,
            ship_x,
            ship_y,
            ship_ramp: self.ship.ramp(),
            ship_roll: self.ship.roll_frame() as i32,
            scroll_accums,
            speed_level: self.camera_y as u16,
        }
    }

    /// Route a key press into the open menu and carry out what it asks for.
    fn menu_key(&mut self, key: Key, output: &mut SceneOutput) {
        let Some(menu) = &mut self.menu else {
            return;
        };

        match menu.handle_key(key) {
            None => {}
            Some(MenuRequest::Resume) => {
                self.menu = None;

                // The original's dispatcher clears the freeze wholesale when
                // the menu handler returns (file 0xbdc1), so closing the menu
                // resumes play directly — even when it was opened from the
                // GET READY freeze, whose fire-wait is skipped. A death
                // explosion keeps playing out (dying is not the freeze flag).
                if matches!(self.flow, Flow::GetReady { .. }) {
                    self.flow = Flow::Running;
                }
            }
            Some(MenuRequest::NewGame) => {
                // Exit status 4: START.EXE restarts the chain fresh.
                output.transition = Some(Transition::To(SceneId::Level {
                    level: Level::L1,
                    handoff: Handoff::new_game(),
                }));
            }
            Some(MenuRequest::Quit) => {
                // Exit status 2: back to the front-end menu.
                output.transition = Some(Transition::To(SceneId::MainMenu));
            }
            Some(MenuRequest::Save(slot)) => self.menu_save(slot),
            Some(MenuRequest::Load(slot)) => self.menu_load(slot, output),
        }
    }

    /// Write the running level into a slot.
    fn menu_save(&mut self, slot: usize) {
        let save = self.to_save();

        if let Some(menu) = &mut self.menu {
            menu.save_to(slot, &save);
        }
    }

    /// Load a slot: in place when the save is for this level (the world
    /// swaps under the open menu, the way the original's same-WAD load
    /// does), through the launcher otherwise (the original bounces back to
    /// START.EXE with the corrected level byte).
    fn menu_load(&mut self, slot: usize, output: &mut SceneOutput) {
        let Some(menu) = &self.menu else {
            return;
        };

        match menu.load_from(slot) {
            Ok(save) if save.level == self.level => {
                let mut menu = self.menu.take().expect("the menu is open");
                menu.loaded();

                *self = LevelScene::from_save(Rc::clone(&self.assets), save);
                // The track keeps playing under the menu; the resume restarts
                // it from the top (the original's reload at file 0xfe4), which
                // the rebuilt scene's pending-start does once play resumes.
                self.music_stopped = true;
                self.menu = Some(menu);
            }
            Ok(_) => {
                output.transition = Some(Transition::To(SceneId::LoadGame { slot }));
            }
            Err(error) => tracing::warn!("loading slot {}: {error:#}", slot + 1),
        }
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

        // L1's form-2 boss arg; the skip stops there so the final fight
        // stays playable.
        const BOSS_FORM_2: u16 = 23;

        self.flow = Flow::Running;
        let mut scratch = Vec::new();

        for _ in 0..ticks {
            if let Some(spawns) = &mut self.spawns {
                let is_l1 = self.assets.spawn_ai == Some(crate::levels::SpawnAi::L1);

                if is_l1 && spawns.entities.iter().any(|e| e.arg == BOSS_FORM_2) {
                    tracing::info!("skip stopped at the final boss form");
                    break;
                }

                if spawns.gate_holds() {
                    let (gate_min, gate_max) = spawns.combat.gate_release;
                    let mut released = 0;

                    for entity in &mut spawns.entities {
                        if entity.health > 0 && (gate_min..=gate_max).contains(&entity.sprite) {
                            entity.health = 0;
                            released += 1;
                        }
                    }

                    // Nothing released means a boss holds the gate: "defeat"
                    // it by dropping it to L1's dying threshold (close enough
                    // to a kill on every level's boss).
                    if released == 0 {
                        let boss_kind = spawns.combat.level_end_sprite;

                        for entity in &mut spawns.entities {
                            if entity.health > 0x1388 && entity.kind >= boss_kind {
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

    /// Cycle to the next weapon. The pod replays when the firing weapon
    /// actually resolves to it, not on the keypress (see `advance`).
    fn cycle_weapon(&mut self) {
        self.state.cycle_weapon();
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
    /// Whatever was playing stops first: START.EXE calls into the resident
    /// CD driver before each level launch, so the previous track's tail
    /// (which runs under the transition movie) dies at the level's GET
    /// READY. The start itself waits for the first GET READY dismissal: the
    /// original bakes a pending-start flag (`cs:0x736c`) that the first
    /// unfreeze consumes (file `0x9e65`), so the opening GET READY is silent
    /// and respawn freezes don't restart the track.
    fn advance_music(&mut self, ticks: u32, audio: &mut Vec<AudioCommand>) {
        if !self.music_started {
            if !self.music_stopped {
                self.music_stopped = true;
                audio.push(AudioCommand::StopMusic);
            }

            if !matches!(self.flow, Flow::Running) {
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
            // The wear-off fade reads the timer before the decrement (the
            // original's ISR runs the DAC block first), so its last write
            // happens with one tick remaining.
            ship::invincibility_fade(&mut self.frame.palette, self.state.invincible_ticks);
            self.state.tick();
            self.run_combat(audio);

            // The death sequence: the world keeps running, the ship doesn't.
            // When the explosion finishes, the life comes off and the level
            // freezes into the respawn GET READY, or exits when that was the
            // last one (the original's respawn handler, file 0x9d84).
            if let Flow::Dying { ticks_left } = &mut self.flow {
                *ticks_left -= 1;

                if *ticks_left == 0 {
                    let respawn_invincibility = self.assets.combat.respawn_invincibility;

                    if self.state.lose_life(respawn_invincibility) == HitOutcome::GameOver {
                        self.flow = Flow::GameOver;
                    } else {
                        self.ship = Ship::new(self.assets.ship);
                        // The shield bubble tracks the invincibility timer
                        // the respawn handler arms.
                        self.ship.arm_shield(i32::from(respawn_invincibility));

                        // A race respawn restarts the course: the original
                        // restores the ISR-mutated table, rewinds the spawn
                        // cursor, wipes the live entities, and zeroes the
                        // scroll accumulators. Effects and live player shots
                        // carry over; speed (the camera) persists.
                        if self.assets.combat.course_restart {
                            let mut fresh = Spawns::new(
                                self.assets.spawns.records(&self.assets.wad, clock_seed()),
                                self.assets.spawn_ai,
                                self.assets.combat,
                            );

                            if let Some(old) = self.spawns.take() {
                                fresh.effects = old.effects;
                            }

                            self.spawns = Some(fresh);
                            self.background_scroll = self.assets.background.scroll();
                            self.scenery_scroll = self.assets.scenery.scroll();
                        }

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

            // The resolve changing the firing weapon plays the switch sound
            // and replays the pod animation (file 0xae59); both wait out a
            // held burst because the resolve is gated on fire released.
            if sounds.switched {
                self.sfx.weapon_switched(&self.assets.sfx, audio);
                self.pod_frame = 0;
                self.pod_ticks = 0;
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
        let (ship_x, ship_y) = self.ship.position();
        spawns.step_movement(
            wad,
            PlayerInput {
                x: ship_x,
                y: ship_y,
                firing_plasma: self.weapons.firing() == ActiveWeapon::Selected(Weapon::Plasma),
                steering: self.held.left || self.held.right,
            },
        );

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
            combat::ship_rects(
                wad,
                cs_base,
                self.assets.combat.ship_rect_table,
                self.ship.roll_frame(),
                ship_x,
                ship_y,
            )
        };

        for _ in 0..combat::enemy_shots_vs_ship(spawns, wad, cs_base, &rects) {
            let firing = self.weapons.firing();
            let outcome = self.state.take_hit(Severity::Bullet, firing);
            events.ship = merge_outcome(events.ship, outcome);

            if outcome == HitOutcome::Absorbed {
                // A drain to zero loses the weapon mid-hold (the original's
                // immediate revert to the minigun, with its own sound).
                let lost = matches!(firing, ActiveWeapon::Selected(weapon)
                    if self.state.level(weapon).get() == 0);

                if lost {
                    self.weapons.weapon_lost();
                    self.sfx.weapon_lost(&self.assets.sfx, audio);
                } else {
                    self.sfx.weapon_drained(&self.assets.sfx, audio);
                }
            }
        }

        combat::body_contact(
            spawns,
            &rects,
            &mut self.state,
            self.weapons.firing(),
            wad,
            cs_base,
            &mut events,
        );

        self.state.add_score(events.score);

        // The shooters end on a boss death (reap), the races on the finish
        // entity's AI flag; both surface as the spawn layer's level_end.
        if (events.level_end || spawns.level_end) && self.level_end_countdown.is_none() {
            tracing::info!(score = self.state.score, "level complete");
            self.level_end_countdown = Some(460);
        }

        if let Some(sound) = spawns.boss_explosion.take() {
            self.sfx.boss_explosion(sound, &self.assets.sfx, audio);
        }

        for slot in spawns.ai_sounds.drain(..) {
            self.sfx.ai_sound(slot, &self.assets.sfx, audio);
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
        // The panel (pod, overlay) shows the resolved firing weapon, frozen
        // across a held burst, not the instantaneous selection.
        let active = self.weapons.firing();

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

        // While the ship is dying, its whole draw block is skipped (the
        // `cs:0x46b2` gate at file 0xb952): no orbs, no pose, no muzzle
        // flash, no shield. Only the explosion draws at the ship position
        // (the gate's target, file 0xbafd). Live shots keep drawing in
        // their own ungated pass.
        let dying_frame = match self.flow {
            Flow::Dying { ticks_left } => Some(((DEATH_TICKS - ticks_left) / 4) as usize),
            _ => None,
        };

        self.weapons.render(
            &self.assets.fire_sprites,
            &mut self.frame,
            self.camera_y,
            dying_frame.is_none(),
        );

        if let Some(frame_index) = dying_frame {
            if let Some(sprite) = self.assets.ship_explosion.get(frame_index) {
                let (x, y) = self.ship.position();
                self.frame.blit_transparent(
                    &sprite.pixels,
                    sprite.size,
                    playfield::LEFT + x,
                    y - self.camera_y,
                );
            }
        } else {
            self.ship.render(
                &self.assets.ship_frames,
                &self.assets.shield_frames,
                &mut self.frame,
                self.camera_y,
            );

            self.weapons.render_flash(
                &self.assets.fire_sprites,
                &mut self.frame,
                self.ship.position(),
                self.ship.roll_frame(),
                &self.assets.barrel_offsets,
                self.camera_y,
            );
        }

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

        // The menu draws over the dimmed playfield in place of GET READY
        // (the original captures and dims the frozen frame once at freeze
        // entry, file 0xb22d; the menu page never carries the text).
        if self.menu.is_some() {
            self.dim_playfield();

            if let Some(menu) = &self.menu {
                menu.render(&self.assets.font, &self.assets.dim_table, &mut self.frame);
            }
        } else if matches!(self.flow, Flow::GetReady { .. }) {
            self.dim_playfield();
            self.assets.font.draw_into(
                &mut self.frame.image,
                GET_READY_POS.0,
                GET_READY_POS.1,
                GET_READY_TEXT,
            );
        }
    }

    /// One tick of the freeze pulse (the stepper every WAD's per-tick input
    /// block falls into while frozen, L1 file `0xb42d`; DAC upload in the
    /// frozen ISR, L1 `0x9482`): lerp the four cursor entries between
    /// [`PULSE_SOURCE`] and their full colors by `v / 0x40`, then bounce
    /// `v`. The lerp reads `v` before the step, and the last
    /// blend persists after unfreeze (nothing restores the entries; the WAD
    /// palette only bakes placeholders there).
    fn pulse_freeze_palette(&mut self) {
        for (entry, target) in PULSE_ENTRIES {
            let mut blended = [0u8; 3];

            for channel in 0..3 {
                let from = i32::from(PULSE_SOURCE[channel]);
                let delta = i32::from(target[channel]) - from;
                blended[channel] = (from + delta * self.pulse_value / 0x40) as u8;
            }

            self.frame.palette.colors[entry] =
                Rgb::from_vga_6bit(blended[0], blended[1], blended[2]);
        }

        if self.pulse_rising {
            self.pulse_value += PULSE_STEP;

            if self.pulse_value >= PULSE_MAX {
                self.pulse_rising = false;
            }
        } else {
            self.pulse_value -= PULSE_STEP;

            if self.pulse_value <= PULSE_MIN {
                self.pulse_rising = true;
            }
        }
    }

    /// The GET READY freeze darkens the playfield but not the panel: every
    /// playfield pixel is remapped through the level's third-brightness
    /// table before the text draws over it. Engine-common: the shooters
    /// remap at the frozen draw (L1 file `0xe60f`), the races inside their
    /// freeze capture (L2 file `0xb2fa`, table built at `0xb0dc` with the
    /// same divisor 3).
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
                // The open menu takes every key press; releases still reach
                // the held-key state so nothing sticks across the freeze.
                KeyEvent::Pressed(key) if self.menu.is_some() => {
                    self.menu_key(key, &mut output);
                }
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
                    // Esc freezes the level and opens the in-game menu. The
                    // original's gate (file 0x91bb) blocks it during the win
                    // flyout; game over transitions out by itself.
                    Key::Esc => {
                        if self.level_end_countdown.is_none()
                            && !matches!(self.flow, Flow::GameOver)
                        {
                            self.menu = Some(InGameMenu::new(crate::savestore::open_or_warn()));
                        }
                    }
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
        // respawn handler's two key loops at file 0x9d84). The open menu
        // owns the keys, so the wait pauses under it.
        if self.menu.is_none()
            && let Flow::GetReady { fire_released } = &mut self.flow
        {
            if !self.fire_held {
                *fire_released = true;
            } else if *fire_released {
                self.flow = Flow::Running;
            }
        }

        // The music runs off the timer ISR in the original, so neither the
        // dev pause nor any freeze (GET READY, the menu) stops its loop
        // countdown; the track keeps playing under the menu.
        self.advance_music(ticks, &mut output.audio);

        if let Some(menu) = &mut self.menu {
            menu.advance(ticks);
        }

        // The menu freezes everything, the death explosion included (the
        // original's freeze halts the whole frame loop).
        let frozen =
            matches!(self.flow, Flow::GetReady { .. } | Flow::GameOver) || self.menu.is_some();

        // The freeze pulse runs while the freeze flag is up, whatever
        // handler owns the freeze (GET READY or the menu): the per-tick
        // input block falls through into the stepper in every WAD (L1 file
        // 0xb425), and the frozen ISR uploads the blend each tick.
        if frozen {
            for _ in 0..ticks {
                self.pulse_freeze_palette();
            }
        }

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
        // and it flies off the right edge. Then the level hands its
        // writeback to the next one in the chain.
        if let Some(countdown) = &mut self.level_end_countdown {
            *countdown = countdown.saturating_sub(ticks);

            if *countdown < 300 {
                self.ship.fly_out();
            }

            if *countdown == 0 {
                output.transition = Some(Transition::To(SceneId::LevelTransition {
                    after: self.level,
                    handoff: self.state.handoff(),
                }));
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
    use openprototype_core::{PerWeapon, WeaponLevel};

    /// A scene past the GET READY freeze, so ticks advance the world.
    fn test_scene() -> LevelScene {
        let mut scene = LevelScene::new(
            Rc::new(test_level_assets()),
            Level::L1,
            Handoff::new_game(),
            0,
        );
        scene.flow = Flow::Running;

        scene
    }

    /// A running scene with every weapon charged, for the weapon-cycling and
    /// pod-replay tests (a fresh start carries only the bare chaingun).
    fn charged_scene() -> LevelScene {
        let mut scene = test_scene();
        scene.state.weapons = PerWeapon::splat(WeaponLevel::new(WeaponLevel::MAX));

        // One idle tick so the firing weapon re-resolves to the charged
        // selection, then settle the pod that the resolve replayed.
        scene.update(TICK, &[]);
        scene.pod_frame = POD_SETTLED_FRAME;

        scene
    }

    #[test]
    fn starts_bare_with_the_spawn_shield_and_the_pod_settled() {
        let scene = test_scene();

        for weapon in Weapon::ALL {
            assert_eq!(scene.state.level(weapon).get(), 0);
        }

        // The level start runs the spawn handler's respawn path, so the
        // spawn shield is armed; the weapons start bare.
        assert_eq!(
            scene.state.invincible_ticks,
            scene.assets.combat.respawn_invincibility
        );
        assert_eq!(scene.state.active_weapon(), ActiveWeapon::Chaingun);
        assert_eq!(scene.pod_frame, POD_SETTLED_FRAME);
        assert_eq!(scene.camera_y, 0);
    }

    #[test]
    fn a_savegame_restores_the_level_mid_action() {
        let save =
            crate::savegame::SaveGame::decode(include_bytes!("../../tests/fixtures/l2-race.psg"))
                .expect("the ground-truth fixture decodes");
        let expected = save.clone();

        let scene = LevelScene::from_save(Rc::new(test_level_assets()), save);

        assert_eq!(scene.state, expected.state);
        assert!(
            matches!(scene.flow, Flow::GetReady { .. }),
            "a load resumes frozen on GET READY"
        );
        assert_eq!(scene.ship.position(), (120, 23));
        assert_eq!(scene.ship.roll_frame(), 8);
        assert_eq!(scene.camera_y, 0);

        let spawns = scene.spawns.as_ref().expect("the spawn layer restores");
        assert_eq!(spawns.entities.len(), 5);
        assert_eq!(spawns.entities[0].kind, 0x3e1c);
        assert_eq!(spawns.entities[0].health, 31_964);
    }

    #[test]
    fn to_save_round_trips_the_restored_level() {
        let save =
            crate::savegame::SaveGame::decode(include_bytes!("../../tests/fixtures/l2-race.psg"))
                .expect("the ground-truth fixture decodes");
        let expected = save.clone();

        let scene = LevelScene::from_save(Rc::new(test_level_assets()), save);
        let mut resaved = scene.to_save();

        // The synthetic assets carry no scenery layer, so the nebula
        // accumulator has nowhere to live across the round trip; with real
        // assets it restores like the background's (same mechanism, see the
        // scenery tests).
        resaved.scroll_accums[0] = expected.scroll_accums[0];

        assert_eq!(resaved, expected);
    }

    #[test]
    fn to_save_round_trips_a_restored_shooter_level() {
        let save = crate::savegame::SaveGame::decode(include_bytes!("../../tests/fixtures/l1.psg"))
            .expect("the ground-truth fixture decodes");
        let expected = save.clone();

        // The synthetic assets' Canyon background has L1's seven strips, so
        // the strip accumulators carry across the round trip; the scenery
        // and derived slots have no synthetic state to live in.
        let scene = LevelScene::from_save(Rc::new(test_level_assets()), save);
        let mut resaved = scene.to_save();
        resaved.scroll_accums[..3].copy_from_slice(&expected.scroll_accums[..3]);

        assert_eq!(resaved, expected);
    }

    #[test]
    fn the_menu_saves_a_shooter_level_into_a_loadable_slot() {
        let (_dir, store) = temp_store();
        let mut scene = LevelScene::from_save(
            Rc::new(test_level_assets()),
            crate::savegame::SaveGame::decode(include_bytes!("../../tests/fixtures/l1.psg"))
                .expect("the ground-truth fixture decodes"),
        );
        scene.menu = Some(InGameMenu::new(Some(store)));

        // SAVE GAME (third item), slot 1.
        press(&mut scene, Key::Down);
        press(&mut scene, Key::Down);
        press(&mut scene, Key::Enter);
        press(&mut scene, Key::Enter);

        let menu = scene.menu.as_ref().expect("the menu shows the toast");
        let written = menu.load_from(0).expect("the slot reads back");
        assert_eq!(written.level, Level::L1);
        assert_eq!(written.state.score, 25_170);
    }

    #[test]
    fn the_level_end_hands_the_writeback_to_the_transition() {
        let mut scene = test_scene();
        scene.state.score = 12_345;
        scene.level_end_countdown = Some(1);

        let output = scene.update(TICK, &[]);

        assert_eq!(
            output.transition,
            Some(Transition::To(SceneId::LevelTransition {
                after: Level::L1,
                handoff: scene.state.handoff(),
            }))
        );
    }

    #[test]
    fn a_one_life_carry_without_the_refund_is_game_over_on_entry() {
        let mut carry = Handoff::new_game();
        carry.lives = openprototype_core::Lives::new(1);
        carry.score = 5_000;

        let mut scene = LevelScene::new(Rc::new(test_level_assets()), Level::L3, carry, 0);

        assert_eq!(
            scene.update(TICK, &[]).transition,
            Some(Transition::To(SceneId::GameOver { score: 5_000 }))
        );
    }

    #[test]
    fn shift_cycles_the_weapon_and_the_resolve_restarts_the_pod() {
        let mut scene = charged_scene();
        assert_eq!(
            scene.state.active_weapon(),
            ActiveWeapon::Selected(Weapon::Multishot)
        );

        // The press itself only moves the selection; the pod replays when
        // the firing weapon re-resolves on the next tick (fire not held).
        scene.update(Duration::ZERO, &[KeyEvent::Pressed(Key::Shift)]);
        assert_eq!(scene.pod_frame, POD_SETTLED_FRAME);

        scene.update(TICK, &[]);

        assert_eq!(
            scene.state.active_weapon(),
            ActiveWeapon::Selected(Weapon::Burning)
        );
        assert_eq!(scene.pod_frame, 0);
    }

    #[test]
    fn a_held_burst_freezes_the_panels_firing_weapon() {
        let mut scene = charged_scene();

        // Fire held: the selection moves but the resolve (and the pod
        // replay) wait for the release.
        scene.update(TICK, &[KeyEvent::Pressed(Key::Ctrl)]);
        scene.update(TICK, &[KeyEvent::Pressed(Key::Shift)]);
        assert_eq!(
            scene.weapons.firing(),
            ActiveWeapon::Selected(Weapon::Multishot)
        );
        assert_eq!(scene.pod_frame, POD_SETTLED_FRAME);

        scene.update(TICK, &[KeyEvent::Released(Key::Ctrl)]);
        scene.update(TICK, &[]);
        assert_eq!(
            scene.weapons.firing(),
            ActiveWeapon::Selected(Weapon::Burning)
        );
        assert_eq!(scene.pod_frame, 0);
    }

    #[test]
    fn the_pod_animation_advances_to_settled_then_stops() {
        let mut scene = charged_scene();
        scene.update(TICK, &[KeyEvent::Pressed(Key::Shift)]);
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

        // The first frame stops the launcher's leftover music and starts
        // the track, even without an elapsed tick.
        let output = scene.update(Duration::ZERO, &[]);
        assert_eq!(
            output.audio,
            vec![AudioCommand::StopMusic, AudioCommand::PlayTrack(track)]
        );

        // Nothing replays while the countdown runs (the test assets' track
        // is 10 ticks long).
        let output = scene.update(TICK * 10, &[]);
        assert_eq!(output.audio, vec![]);

        // The countdown underflows one tick past the length: restart.
        let output = scene.update(TICK, &[]);
        assert_eq!(output.audio, vec![AudioCommand::PlayTrack(track)]);
    }

    /// One key press with no elapsed time.
    fn press(scene: &mut LevelScene, key: Key) -> SceneOutput {
        scene.update(Duration::ZERO, &[KeyEvent::Pressed(key)])
    }

    /// A temp-dir slot store and its directory guard.
    fn temp_store() -> (tempfile::TempDir, crate::savestore::SaveStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::savestore::store_at(dir.path().to_path_buf());

        (dir, store)
    }

    fn race_fixture() -> crate::savegame::SaveGame {
        crate::savegame::SaveGame::decode(include_bytes!("../../tests/fixtures/l2-race.psg"))
            .expect("the ground-truth fixture decodes")
    }

    #[test]
    fn esc_opens_the_menu_and_freezes_the_world() {
        let mut scene = test_scene();
        press(&mut scene, Key::Esc);

        assert!(scene.menu.is_some());

        // Nothing advances under the menu: the scroll holds where it froze.
        let column = scene.background_scroll.pixel_column(0);
        scene.update(TICK * 10, &[]);
        assert_eq!(scene.background_scroll.pixel_column(0), column);
        assert!(matches!(scene.flow, Flow::Running));
    }

    #[test]
    fn esc_is_blocked_during_the_win_flyout() {
        let mut scene = test_scene();
        scene.level_end_countdown = Some(100);

        press(&mut scene, Key::Esc);

        assert!(scene.menu.is_none());
    }

    #[test]
    fn the_menu_quit_item_returns_to_the_front_end() {
        let mut scene = test_scene();
        press(&mut scene, Key::Esc);

        // Up wraps from NEW GAME to QUIT.
        press(&mut scene, Key::Up);
        let output = press(&mut scene, Key::Enter);

        assert_eq!(output.transition, Some(Transition::To(SceneId::MainMenu)));
    }

    #[test]
    fn the_menu_new_game_item_restarts_the_chain_fresh() {
        let mut scene = test_scene();
        scene.state.score = 9_999;
        press(&mut scene, Key::Esc);

        let output = press(&mut scene, Key::Enter);

        assert_eq!(
            output.transition,
            Some(Transition::To(SceneId::Level {
                level: Level::L1,
                handoff: Handoff::new_game(),
            }))
        );
    }

    #[test]
    fn an_inert_item_keeps_the_menu_open() {
        let mut scene = test_scene();
        press(&mut scene, Key::Esc);

        // Down 3x lands on GRAPHICS...; Enter does nothing yet.
        for _ in 0..3 {
            press(&mut scene, Key::Down);
        }
        let output = press(&mut scene, Key::Enter);

        assert_eq!(output.transition, None);
        assert!(scene.menu.is_some());
    }

    #[test]
    fn closing_the_menu_resumes_play_directly_even_from_get_ready() {
        // A fresh level sits frozen on GET READY.
        let mut scene = LevelScene::new(
            Rc::new(test_level_assets()),
            Level::L1,
            Handoff::new_game(),
            0,
        );
        assert!(matches!(scene.flow, Flow::GetReady { .. }));

        press(&mut scene, Key::Esc);
        assert!(scene.menu.is_some());
        press(&mut scene, Key::Esc);

        // The original's dispatcher clears the freeze wholesale, skipping
        // the GET READY fire-wait.
        assert!(scene.menu.is_none());
        assert!(matches!(scene.flow, Flow::Running));
    }

    #[test]
    fn saving_writes_the_slot_and_round_trips() {
        let (_dir, store) = temp_store();
        let mut scene = LevelScene::from_save(Rc::new(test_level_assets()), race_fixture());
        scene.menu = Some(InGameMenu::new(Some(store)));

        // SAVE GAME (third item), then slot 2.
        press(&mut scene, Key::Down);
        press(&mut scene, Key::Down);
        press(&mut scene, Key::Enter);
        press(&mut scene, Key::Down);
        press(&mut scene, Key::Enter);

        let menu = scene.menu.as_ref().expect("the menu shows the toast");
        let written = menu.load_from(1).expect("the slot reads back");
        assert_eq!(written, scene.to_save());
    }

    #[test]
    fn an_in_place_load_swaps_the_world_under_the_menu() {
        let (_dir, store) = temp_store();
        store.save(0, &race_fixture()).unwrap();

        let mut scene = LevelScene::new(
            Rc::new(test_level_assets()),
            Level::L2,
            Handoff::new_game(),
            0,
        );
        scene.menu = Some(InGameMenu::new(Some(store)));

        // LOAD GAME (second item), then slot 1.
        press(&mut scene, Key::Down);
        let output = press(&mut scene, Key::Enter);
        assert_eq!(output.transition, None);
        let output = press(&mut scene, Key::Enter);

        // The save is for this level: the world swaps in place, the menu
        // stays open on its toast, and the track plays on until the resume
        // restarts it.
        assert_eq!(output.transition, None);
        assert_eq!(scene.state.score, 25_000);
        assert!(scene.menu.is_some());
        assert!(scene.music_stopped);

        // The toast blocks keys while it shows; once it expires, resuming
        // starts the (restarted) track without a stop first, in the same
        // update that closes the menu.
        press(&mut scene, Key::Esc);
        assert!(scene.menu.is_some(), "the toast swallows Esc");

        scene.update(TICK * 84, &[]);
        let output = press(&mut scene, Key::Esc);
        assert_eq!(
            output.audio,
            vec![AudioCommand::PlayTrack(scene.assets.music.track)]
        );
    }

    #[test]
    fn a_cross_level_load_relaunches_through_the_app() {
        let (_dir, store) = temp_store();
        store.save(2, &race_fixture()).unwrap();

        // The running level is L1; the slot holds an L2 save.
        let mut scene = test_scene();
        scene.menu = Some(InGameMenu::new(Some(store)));

        press(&mut scene, Key::Down);
        press(&mut scene, Key::Enter);
        press(&mut scene, Key::Down);
        press(&mut scene, Key::Down);
        let output = press(&mut scene, Key::Enter);

        assert_eq!(
            output.transition,
            Some(Transition::To(SceneId::LoadGame { slot: 2 }))
        );
    }

    #[test]
    fn the_freeze_pulses_the_cursor_palette() {
        let mut scene = LevelScene::new(
            Rc::new(test_level_assets()),
            Level::L1,
            Handoff::new_game(),
            0,
        );
        assert!(matches!(scene.flow, Flow::GetReady { .. }));

        // The first frozen tick lerps at the baked level 0x30: white blends
        // to 0x3f*0x30/0x40 over the shared (0, 0, 0x20) endpoint.
        scene.update(TICK, &[]);
        assert_eq!(
            scene.frame.palette.colors[0xff],
            Rgb::from_vga_6bit(0x2f, 0x2f, 0x37)
        );

        // The pulse heads down: the next tick lerps at 0x2e.
        scene.update(TICK, &[]);
        assert_eq!(
            scene.frame.palette.colors[0xff],
            Rgb::from_vga_6bit(0x2d, 0x2d, 0x36)
        );
    }
}

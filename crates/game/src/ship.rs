//! The player ship: movement, the barrel-roll animation, the camera coupling,
//! and the spawn shield.
//!
//! Reverse-engineered from `LEVEL_1.WAD`'s per-tick handler (file `0xb4c0`);
//! the engine code is the same in every level, with the per-level constants
//! (idle frame, vertical clamps, spawn shield duration) byte-verified in each
//! WAD and carried in [`ShipData`]. All movement is digital, 2 pixels per
//! tick per held direction, no inertia. Positions are in the original's
//! compose-buffer space: x is window-relative (the playfield's 288 columns
//! start at screen x 16), y is camera-inclusive, so the ship's screen row is
//! `y - camera` and the ship rides up the screen as the camera pans down.
//!
//! The camera moves only while a vertical key is held: flying up with the ship
//! in the top band (`y <= 50`) pans up toward the level's `camera_min`, flying
//! down with the ship past `y >= 60` pans down toward 32, one row per tick.
//!
//! The `PTURN1.BN1` frames are a 27-frame barrel-roll cycle plus alternate
//! idle frames. The roll offset (`cs:[0x2664]`, here a frame index) advances
//! every 2nd tick: down or right roll forward, up rolls backward, and with no
//! vertical input it returns to the level's idle frame the short way around
//! the cycle (per-level: most idle top-down on frame 0, L3/L5 side-on at 21;
//! see [`ShipData`]). While idle, a free-running 5-phase counter
//! (`cs:[0x2682]`) swaps in the level's alternate frame on phases 3 and 4,
//! the exhaust flicker.
//!
//! Spawning (`cs:[0x2642]` ramp) flies the ship in from the left at +2/tick
//! with input ignored until the ramp counter reaches 10; the level-end
//! flyout reuses the same ramp by re-pinning it each tick ([`Ship::fly_out`])
//! so the ship exits right under locked controls. The shield bubble lights
//! when [`Ship::arm_shield`] mirrors the invincibility timer (`cs:[0x266a]`):
//! every pass through the spawn handler (the level start runs its respawn
//! path too) and the invincibility pickup. It animates over
//! [`SHIELD_FRAMES`] frames, 4 ticks each (`cs:[0x8f5c]`/`[0x8f5e]`), and
//! draws at the ship position offset by `(+4, +6)`.

use openprototype_core::framebuffer::Framebuffer;
use prototype_formats::bin::SpriteSheet;
use prototype_formats::{Palette, Rgb};

use crate::assets::OverlaySprite;
use crate::levels::ShipData;
use crate::playfield;

/// Frames in the barrel-roll cycle (`0x1e6` catalog bytes / `0x12` per frame).
const ROLL_FRAMES: i32 = 27;
/// The roll returns to idle by the shorter way around the cycle: backward when
/// idle is at most this many frames behind, forward otherwise. This single
/// rule reproduces every level's branch structure (L1's `0xea` midpoint around
/// idle 0, L3/L5's `0x7e`/`0x18c` split around idle 21).
const ROLL_RETURN_MIDPOINT: i32 = 13;
/// The idle exhaust flicker: a free-running 5-phase counter shows the
/// alternate frame on phases 3 and 4.
const IDLE_PHASES: u8 = 5;
const IDLE_FLICKER_FROM: u8 = 3;

/// Shield animation frames (`cs:[0x8f5c]` cycles offsets `0..0x58` step 8).
pub const SHIELD_FRAMES: usize = 11;
/// Ticks each shield frame holds (`cs:[0x8f5e]`).
const SHIELD_FRAME_TICKS: u8 = 4;
/// The shield sprite's offset from the ship position.
const SHIELD_OFFSET: (i32, i32) = (4, 6);

/// Spawn state: position, the ramp counter, and its input-unlock threshold.
const SPAWN_X: i32 = -60;
const SPAWN_Y: i32 = 45;
const SPAWN_RAMP: i32 = -80;
const RAMP_DONE: i32 = 10;

/// The level-end flyout re-pins the ramp to this every tick (`cs:[0x2642]` =
/// `0xfed4`), so the auto-drift branch keeps running and the ship leaves the
/// right edge under locked controls.
const FLYOUT_RAMP: i32 = -300;

/// Horizontal bounds (`cmp` guards before each 2-pixel step), the same in
/// every level; the vertical bounds are per-level ([`ShipData::y_min`]).
const X_MIN: i32 = -12;
const X_MAX: i32 = 230;

/// Camera coupling: flying up pans the camera up while the ship is at or above
/// this row.
const PAN_UP_BELOW: i32 = 50;
/// Flying down pans the camera down while the ship is at or below this row.
const PAN_DOWN_ABOVE: i32 = 60;
/// The camera's lower stop (the upper stop is the level's `camera_min`).
const CAMERA_MAX: i32 = 32;

/// Which flight keys are currently held, the port's stand-in for the
/// original ISR's key-state flags.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HeldKeys {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

/// The player ship's state machine. Advance it with [`update`](Ship::update)
/// once per logic tick and composite it with [`render`](Ship::render).
pub struct Ship {
    /// The level-flight frame the roll returns to (per-level: 0 top-down, 21
    /// side view; [`ShipData::idle_frame`]).
    idle_frame: i32,
    /// The idle exhaust-flicker alternate frame ([`ShipData::flicker_frame`]).
    flicker_frame: usize,
    /// The level's vertical clamps ([`ShipData::y_min`]/[`ShipData::y_max`]).
    y_min: i32,
    y_max: i32,
    /// Window-relative x of the frame's left cell.
    x: i32,
    /// Camera-inclusive y (buffer row); screen row is `y - camera`.
    y: i32,
    /// Spawn fly-in counter; input is ignored until it reaches [`RAMP_DONE`].
    ramp: i32,
    /// Barrel-roll frame index, `0..ROLL_FRAMES` (`cs:[0x2664]` over `0x12`).
    roll: i32,
    /// Divider so the roll advances every 2nd tick (`cs:[0x2681]`).
    roll_divider: u8,
    /// Free-running idle-flicker phase, `0..IDLE_PHASES` (`cs:[0x2682]`).
    idle_phase: u8,
    /// Remaining shield ticks (`cs:[0x266a]`).
    shield_ticks: i32,
    /// Current shield animation frame (`cs:[0x8f5c]` over 8).
    shield_frame: usize,
    /// Ticks left on the current shield frame (`cs:[0x8f5e]`).
    shield_hold: u8,
}

impl Ship {
    /// A freshly spawned ship for a level with the given frame selection. The
    /// roll always starts at frame 0, so a side-view level rolls into its
    /// idle pose during the fly-in.
    pub fn new(ship: ShipData) -> Self {
        Self {
            idle_frame: ship.idle_frame as i32,
            flicker_frame: ship.flicker_frame,
            y_min: ship.y_min,
            y_max: ship.y_max,
            x: SPAWN_X,
            y: SPAWN_Y,
            ramp: SPAWN_RAMP,
            roll: 0,
            roll_divider: 0,
            idle_phase: 0,
            // Unarmed: the scene arms the spawn/respawn shield (and the
            // invincibility pickup relights it) via [`Self::arm_shield`],
            // keeping the duration in one place with the hit-state timer.
            shield_ticks: 0,
            shield_frame: 0,
            shield_hold: SHIELD_FRAME_TICKS,
        }
    }

    /// The ship's window-relative position (for tests and debug overlays).
    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// The current barrel-roll frame, which the fire system uses to index the
    /// barrel-offset table.
    pub fn roll_frame(&self) -> usize {
        self.roll as usize
    }

    /// The spawn fly-in ramp counter, for the savegame snapshot.
    pub fn ramp(&self) -> i32 {
        self.ramp
    }

    /// Arms the shield visual for `ticks` (the invincibility pickup relights
    /// it; the original drives both off the same `cs:0x266a` counter).
    pub fn arm_shield(&mut self, ticks: i32) {
        self.shield_ticks = ticks;
    }

    /// Place the ship from a savegame snapshot: position, the fly-in ramp
    /// counter, and the roll frame. The roll divider and idle phase are
    /// engine scratch the snapshot does not model.
    pub fn restore(&mut self, x: i32, y: i32, ramp: i32, roll: i32) {
        self.x = x;
        self.y = y;
        self.ramp = ramp;
        self.roll = roll.clamp(0, ROLL_FRAMES - 1);
    }

    /// One tick of the level-end flyout: pins the ramp below the unlock
    /// threshold so [`Self::update`] drifts the ship right with input
    /// ignored. The original's flyout loop (file `0xf866`) re-forces
    /// `cs:[0x2642]` every frame for the last 300 of its 460, which is why
    /// this must be called per tick rather than once.
    pub fn fly_out(&mut self) {
        self.ramp = FLYOUT_RAMP;
    }

    /// Reset for a respawn: position, fly-in ramp and roll restart, but the
    /// free-running animation phases carry over -- the original's respawn
    /// never resets the idle-flicker phase, the roll divider, or the shield
    /// animation frame/hold (L1 0xb601-area state untouched by the respawn
    /// handler), and they keep stepping through the death sequence.
    pub fn respawn(&mut self, data: crate::levels::ShipData) {
        let idle_phase = self.idle_phase;
        let roll_divider = self.roll_divider;
        let shield_frame = self.shield_frame;
        let shield_hold = self.shield_hold;

        *self = Ship::new(data);
        self.idle_phase = idle_phase;
        self.roll_divider = roll_divider;
        self.shield_frame = shield_frame;
        self.shield_hold = shield_hold;
    }

    /// One dying-sequence tick: the explosion replaces the ship on screen,
    /// but the original keeps stepping the flicker phase, the shield
    /// timer/animation, and the roll (input gated off, so it levels out).
    pub fn tick_animations(&mut self) {
        self.idle_phase = (self.idle_phase + 1) % IDLE_PHASES;
        self.advance_shield();
        self.advance_roll(HeldKeys::default());
    }

    /// Advance one logic tick: animations, movement, and the camera coupling.
    pub fn update(&mut self, held: HeldKeys, camera: &mut i32, camera_min: i32) {
        self.idle_phase = (self.idle_phase + 1) % IDLE_PHASES;
        self.advance_shield();
        self.advance_roll(held);

        // The spawn fly-in: drift right with input ignored (and no camera
        // coupling) until the ramp counter unlocks.
        if self.ramp < RAMP_DONE {
            self.ramp += 2;
            self.x += 2;

            return;
        }

        if held.right && self.x < X_MAX {
            self.x += 2;
        }

        if held.left && self.x > X_MIN {
            self.x -= 2;
        }

        // The up branch always exits past the down check (L1 file 0xb601..
        // 0xb650, congruent in the race WADs), so with both keys held the
        // ship climbs; left+right DO stack and cancel.
        if held.up {
            if self.y > self.y_min {
                self.y -= 2;
            }

            if self.y <= PAN_UP_BELOW && *camera > camera_min {
                *camera -= 1;
            }
        } else if held.down {
            if self.y < self.y_max {
                self.y += 2;
            }

            if self.y >= PAN_DOWN_ABOVE && *camera < CAMERA_MAX {
                *camera += 1;
            }
        }
    }

    /// Count down the shield and advance its looping animation. The animation
    /// runs whether or not the shield shows, like the original's.
    fn advance_shield(&mut self) {
        if self.shield_ticks > 0 {
            self.shield_ticks -= 1;
        }

        self.shield_hold -= 1;

        if self.shield_hold == 0 {
            self.shield_hold = SHIELD_FRAME_TICKS;
            self.shield_frame = (self.shield_frame + 1) % SHIELD_FRAMES;
        }
    }

    /// Advance the barrel roll every 2nd tick: roll with the held vertical
    /// direction (right also rolls forward), otherwise return to level the
    /// short way around the cycle.
    fn advance_roll(&mut self, held: HeldKeys) {
        self.roll_divider += 1;

        if self.roll_divider < 2 {
            return;
        }

        self.roll_divider = 0;

        if held.down {
            self.roll += 1;
        } else if held.up {
            self.roll -= 1;
        } else if held.right {
            self.roll += 1;
        } else if self.roll != self.idle_frame {
            let backward = (self.roll - self.idle_frame).rem_euclid(ROLL_FRAMES);

            if backward <= ROLL_RETURN_MIDPOINT {
                self.roll -= 1;
            } else {
                self.roll += 1;
            }
        }

        self.roll = self.roll.rem_euclid(ROLL_FRAMES);
    }

    /// The PTURN1 frame to draw this tick.
    fn frame(&self) -> usize {
        if self.roll == self.idle_frame && self.idle_phase >= IDLE_FLICKER_FROM {
            self.flicker_frame
        } else {
            self.roll as usize
        }
    }

    /// Composite the ship (and its shield while one is up) into the playfield.
    ///
    /// Both compose in buffer space like the original (the scene's window mask
    /// crops whatever bleeds past the playfield): screen x is window-relative
    /// plus the window's left edge, screen y subtracts the camera.
    pub fn render(
        &self,
        frames: &SpriteSheet,
        shield_frames: &[OverlaySprite],
        frame: &mut Framebuffer,
        camera: i32,
    ) {
        let screen_x = playfield::LEFT + self.x;
        let screen_y = self.y - camera;

        if let Some(sprite) = frames.sprites.get(self.frame()) {
            frame.blit_transparent(
                &sprite.pixels,
                sprite.size,
                screen_x + sprite.origin.0,
                screen_y + sprite.origin.1,
            );
        }

        if self.shield_ticks > 0
            && let Some(shield) = shield_frames.get(self.shield_frame)
        {
            frame.blit_transparent(
                &shield.pixels,
                shield.size,
                screen_x + SHIELD_OFFSET.0,
                screen_y + SHIELD_OFFSET.1,
            );
        }
    }
}

/// The wear-off fade's palette blocks (6-bit DAC values, 16 entries each;
/// LEVEL_1.WAD file `0x12220`/`0x12250`, byte-identical in all seven WADs):
/// the bubble's normal colors and the dark ramp they fade toward.
const SHIELD_FADE_BASE: [[u8; 3]; 16] = [
    [0x3f, 0x3f, 0x34],
    [0x3f, 0x39, 0x34],
    [0x3f, 0x35, 0x35],
    [0x3f, 0x35, 0x3a],
    [0x3f, 0x36, 0x3f],
    [0x37, 0x2f, 0x3c],
    [0x2c, 0x29, 0x39],
    [0x23, 0x27, 0x36],
    [0x22, 0x22, 0x34],
    [0x25, 0x20, 0x31],
    [0x28, 0x1f, 0x2f],
    [0x2a, 0x1e, 0x2d],
    [0x2b, 0x1d, 0x2a],
    [0x29, 0x1b, 0x25],
    [0x27, 0x1a, 0x20],
    [0x25, 0x19, 0x1b],
];
const SHIELD_FADE_TARGET: [[u8; 3]; 16] = [
    [0x3f, 0x3f, 0x34],
    [0x37, 0x38, 0x2d],
    [0x26, 0x28, 0x1f],
    [0x28, 0x2b, 0x21],
    [0x20, 0x25, 0x1b],
    [0x1a, 0x1f, 0x16],
    [0x14, 0x19, 0x11],
    [0x0f, 0x13, 0x0d],
    [0x0a, 0x0e, 0x09],
    [0x08, 0x0c, 0x07],
    [0x09, 0x0d, 0x07],
    [0x0a, 0x11, 0x08],
    [0x0e, 0x16, 0x09],
    [0x11, 0x18, 0x09],
    [0x01, 0x01, 0x00],
    [0x03, 0x04, 0x01],
];

/// The first palette entry the fade rewrites (the bubble's color band runs
/// `0xe0..0xf0`).
const SHIELD_FADE_FIRST_ENTRY: usize = 0xe0;

/// Cross-fade the bubble's palette band toward the dark wear-off ramp while
/// the invincibility timer runs (the ISR block at LEVEL_1.WAD file `0x9498`,
/// identical in every WAD): `out = base + (target - base) * t / 64` with
/// `t = max(0, (0x80 - ticks) >> 1)`, so the bubble keeps its normal colors
/// until 126 ticks remain and darkens linearly from there.
///
/// Runs on every armed tick, before the timer decrement: the `t = 0` rewrite
/// of the base block is what gives L7's bubble its real colors (that WAD's
/// level palette holds placeholder magenta in this band). Nothing restores
/// the entries at expiry; the bubble stops drawing instead, like the
/// original.
pub fn invincibility_fade(palette: &mut Palette, ticks: u16) {
    if ticks == 0 {
        return;
    }

    let t = ((0x80 - i32::from(ticks)) >> 1).max(0);

    let entries = SHIELD_FADE_BASE.iter().zip(&SHIELD_FADE_TARGET);

    for (index, (base, target)) in entries.enumerate() {
        let mut blended = [0u8; 3];

        for channel in 0..3 {
            let from = i32::from(base[channel]);
            let delta = i32::from(target[channel]) - from;
            // i32 division truncates toward zero, matching the `idiv`.
            blended[channel] = (from + delta * t / 64) as u8;
        }

        palette.colors[SHIELD_FADE_FIRST_ENTRY + index] =
            Rgb::from_vga_6bit(blended[0], blended[1], blended[2]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: HeldKeys = HeldKeys {
        up: false,
        down: false,
        left: false,
        right: false,
    };

    /// L1's ship data (top-down idle, the tight -2..110 vertical range).
    const TOP_DOWN: ShipData = ShipData {
        catalog: 0,
        idle_frame: 0,
        flicker_frame: 27,
        y_min: -2,
        y_max: 110,
        explosion: None,
    };

    /// L5's ship data (side-view idle at 21, flicker on the 29th frame).
    const SIDE_VIEW: ShipData = ShipData {
        catalog: 0,
        idle_frame: 21,
        flicker_frame: 28,
        y_min: -12,
        y_max: 113,
        explosion: None,
    };

    fn held(up: bool, down: bool, left: bool, right: bool) -> HeldKeys {
        HeldKeys {
            up,
            down,
            left,
            right,
        }
    }

    /// Run `ticks` updates against a camera starting at `camera_min`.
    fn run(ship: &mut Ship, keys: HeldKeys, ticks: u32, camera: &mut i32, camera_min: i32) {
        for _ in 0..ticks {
            ship.update(keys, camera, camera_min);
        }
    }

    #[test]
    fn the_spawn_ramp_flies_in_ignoring_input_and_unlocks_at_x_30() {
        let mut ship = Ship::new(TOP_DOWN);
        let mut camera = 0;

        // 44 ticks in, the ramp is still active and input is ignored.
        run(
            &mut ship,
            held(true, false, true, false),
            44,
            &mut camera,
            0,
        );
        assert_eq!(ship.position(), (SPAWN_X + 44 * 2, SPAWN_Y));

        // The 45th tick finishes the ramp; the 46th reacts to input.
        run(&mut ship, NONE, 1, &mut camera, 0);
        assert_eq!(ship.position(), (30, SPAWN_Y));

        run(
            &mut ship,
            held(false, false, true, false),
            1,
            &mut camera,
            0,
        );
        assert_eq!(ship.position(), (28, SPAWN_Y));
    }

    /// A ship past its spawn ramp, level, at (30, 45).
    fn flying_ship() -> Ship {
        let mut ship = Ship::new(TOP_DOWN);
        let mut camera = 0;
        run(&mut ship, NONE, 45, &mut camera, 0);

        ship
    }

    #[test]
    fn movement_clamps_to_the_original_bounds() {
        let mut ship = flying_ship();
        let mut camera = 0;

        run(
            &mut ship,
            held(true, false, true, false),
            200,
            &mut camera,
            0,
        );
        assert_eq!(ship.position(), (X_MIN, TOP_DOWN.y_min - 1));

        run(
            &mut ship,
            held(false, true, false, true),
            200,
            &mut camera,
            0,
        );
        assert_eq!(ship.position(), (X_MAX, TOP_DOWN.y_max + 1));
    }

    #[test]
    fn up_wins_when_up_and_down_are_both_held() {
        let mut ship = flying_ship();
        let mut camera = 0;
        let (_, start_y) = ship.position();

        run(&mut ship, held(true, true, false, false), 5, &mut camera, 0);

        let (_, y) = ship.position();
        assert_eq!(y, start_y - 10);
    }

    #[test]
    fn flying_up_pans_the_camera_only_in_the_top_band() {
        let mut ship = flying_ship();
        let mut camera = 10;

        // The ship spawns at y 45, inside the top band, so the camera pans up
        // one row per tick while up is held, stopping at camera_min.
        run(
            &mut ship,
            held(true, false, false, false),
            6,
            &mut camera,
            4,
        );
        assert_eq!(camera, 4);
    }

    #[test]
    fn flying_down_pans_the_camera_toward_its_stop() {
        let mut ship = flying_ship();
        let mut camera = 0;

        // From y 45, eight down ticks reach y 61; the pan starts at y >= 60,
        // so the camera gains one row on the 8th tick.
        run(
            &mut ship,
            held(false, true, false, false),
            8,
            &mut camera,
            0,
        );
        assert_eq!(ship.position().1, 61);
        assert_eq!(camera, 1);

        run(
            &mut ship,
            held(false, true, false, false),
            100,
            &mut camera,
            0,
        );
        assert_eq!(camera, CAMERA_MAX);
    }

    #[test]
    fn the_roll_advances_every_2nd_tick_and_wraps() {
        let mut ship = flying_ship();
        let mut camera = 0;

        run(
            &mut ship,
            held(true, false, false, false),
            4,
            &mut camera,
            0,
        );
        assert_eq!(ship.roll, ROLL_FRAMES - 2);

        // 27 more roll steps (54 ticks) wrap the full cycle back around.
        run(
            &mut ship,
            held(true, false, false, false),
            54,
            &mut camera,
            0,
        );
        assert_eq!(ship.roll, ROLL_FRAMES - 2);
    }

    #[test]
    fn the_roll_returns_to_level_the_short_way() {
        let mut ship = flying_ship();
        let mut camera = 0;

        // Roll backward by two frames (toward 25), then release: the short way
        // back is forward through the wrap.
        run(
            &mut ship,
            held(true, false, false, false),
            4,
            &mut camera,
            0,
        );
        run(&mut ship, NONE, 2, &mut camera, 0);
        assert_eq!(ship.roll, ROLL_FRAMES - 1);

        run(&mut ship, NONE, 2, &mut camera, 0);
        assert_eq!(ship.roll, 0);

        // Roll forward by two frames; the short way back is backward.
        run(
            &mut ship,
            held(false, true, false, false),
            4,
            &mut camera,
            0,
        );
        run(&mut ship, NONE, 4, &mut camera, 0);
        assert_eq!(ship.roll, 0);
    }

    #[test]
    fn the_idle_flicker_shows_the_alternate_frame_on_phases_3_and_4() {
        let mut ship = flying_ship();
        let mut camera = 0;
        let mut frames = Vec::new();

        for _ in 0..IDLE_PHASES {
            ship.update(NONE, &mut camera, 0);
            frames.push(ship.frame());
        }

        frames.sort_unstable();
        assert_eq!(
            frames
                .iter()
                .filter(|&&f| f == TOP_DOWN.flicker_frame)
                .count(),
            2
        );
        assert_eq!(frames.iter().filter(|&&f| f == 0).count(), 3);
    }

    #[test]
    fn rolling_never_shows_the_idle_flicker() {
        let mut ship = flying_ship();
        let mut camera = 0;

        for _ in 0..20 {
            ship.update(held(false, true, false, false), &mut camera, 0);
            assert_ne!(ship.frame(), TOP_DOWN.flicker_frame);
        }
    }

    #[test]
    fn a_side_view_level_rolls_into_its_idle_pose_and_returns_to_it() {
        let mut ship = Ship::new(SIDE_VIEW);
        let mut camera = 0;

        // The roll starts at 0 and returns to the side view's idle frame 21
        // during the fly-in: backward through the wrap (0 -> 26 -> ... -> 21,
        // matching L5's `<= 0x7e -> sub` branch), 6 steps at 2 ticks each.
        run(&mut ship, NONE, 12, &mut camera, 0);
        assert_eq!(ship.roll, 21);

        run(&mut ship, NONE, 50, &mut camera, 0);
        assert_eq!(ship.roll, 21);

        // 14 down-steps past 21 land on frame 8; from there the way back is
        // forward, matching L5's `0x7e < offset < 0x18c -> add` branch.
        run(
            &mut ship,
            held(false, true, false, false),
            28,
            &mut camera,
            0,
        );
        assert_eq!(ship.roll, 8);
        run(&mut ship, NONE, 2, &mut camera, 0);
        assert_eq!(ship.roll, 9);

        // One frame earlier (7) returns backward through the wrap instead,
        // matching the `<= 0x7e -> sub` branch.
        run(
            &mut ship,
            held(true, false, false, false),
            4,
            &mut camera,
            0,
        );
        assert_eq!(ship.roll, 7);
        run(&mut ship, NONE, 2, &mut camera, 0);
        assert_eq!(ship.roll, 6);
    }

    #[test]
    fn a_side_view_level_flickers_its_own_alternate_frame() {
        let mut ship = Ship::new(SIDE_VIEW);
        let mut camera = 0;
        // Settle on the idle pose, then sample one full flicker period.
        run(&mut ship, NONE, 50, &mut camera, 0);

        let mut frames = Vec::new();

        for _ in 0..IDLE_PHASES {
            ship.update(NONE, &mut camera, 0);
            frames.push(ship.frame());
        }

        assert_eq!(
            frames
                .iter()
                .filter(|&&f| f == SIDE_VIEW.flicker_frame)
                .count(),
            2
        );
        assert_eq!(frames.iter().filter(|&&f| f == 21).count(), 3);
    }

    #[test]
    fn the_shield_animates_on_a_4_tick_cadence_and_expires() {
        let mut ship = Ship::new(TOP_DOWN);
        let mut camera = 0;

        // A fresh spawn carries no shield; arming lights it.
        assert_eq!(ship.shield_ticks, 0);
        ship.arm_shield(348);

        assert_eq!(ship.shield_frame, 0);
        run(&mut ship, NONE, 4, &mut camera, 0);
        assert_eq!(ship.shield_frame, 1);

        // A full loop: 11 frames x 4 ticks.
        run(&mut ship, NONE, 44, &mut camera, 0);
        assert_eq!(ship.shield_frame, 1);

        // The armed shield runs out after its 348 ticks.
        assert!(ship.shield_ticks > 0);
        run(&mut ship, NONE, 300, &mut camera, 0);
        assert_eq!(ship.shield_ticks, 0);
    }

    fn magenta_band_palette() -> Palette {
        let mut palette = Palette::from_vga_6bit(&[0u8; 768]).expect("palette decodes");

        for entry in &mut palette.colors[0xe0..0xf0] {
            *entry = Rgb::from_vga_6bit(0x3f, 0x00, 0x2b);
        }

        palette
    }

    #[test]
    fn the_fade_rewrites_the_base_block_while_the_timer_is_high() {
        // L7's level palette bakes placeholder magenta in the bubble band;
        // the t = 0 regime replaces it with the real colors.
        let mut palette = magenta_band_palette();
        invincibility_fade(&mut palette, 300);

        assert_eq!(palette.colors[0xe1], Rgb::from_vga_6bit(0x3f, 0x39, 0x34));
        assert_eq!(palette.colors[0xef], Rgb::from_vga_6bit(0x25, 0x19, 0x1b));
    }

    #[test]
    fn the_fade_lands_one_step_short_of_the_target_on_the_last_tick() {
        let mut palette = magenta_band_palette();
        invincibility_fade(&mut palette, 1);

        // t = 63: entry 0xee blends 0x27/0x1a/0x20 toward 0x01/0x01/0x00.
        assert_eq!(palette.colors[0xee], Rgb::from_vga_6bit(0x02, 0x02, 0x01));
    }

    #[test]
    fn the_fade_leaves_the_palette_alone_after_expiry() {
        let mut palette = magenta_band_palette();
        let before = palette.clone();
        invincibility_fade(&mut palette, 0);

        assert_eq!(palette, before);
    }
}

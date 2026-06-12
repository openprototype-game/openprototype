//! The race levels' drifting star field.
//!
//! Between the parallax background and the scenery, the original draws planes
//! of 30 single-pixel stars each, every plane with its own leftward speed and
//! palette color (brightness tracks speed for depth). A star whose x drops
//! below 16 pixels respawns 288 pixels to the right, so the field loops over
//! the playfield's middle 288 columns forever.
//!
//! Star placement is generated at level start: the engine rolls `rand(320)`,
//! `rand(160)` per star through the layout PRNG, interleaved across the seeded
//! planes (and skipping unseeded ones, see [`StarPlaneData::seeded`]). Rows at
//! or below the panel are generated but never visible, exactly like the
//! original's work buffer, whose bottom rows are never blitted.

use crate::level::prng::EngineRng;
use crate::levels::StarPlaneData;
use openprototype_core::framebuffer::Framebuffer;

/// Every plane holds 30 stars (`cx = 0x1e` in both the init and draw loops).
const STARS_PER_PLANE: usize = 30;

/// Positions are 1/16-pixel fixed point, like every scroll in the engine.
const SUBPIXEL: i32 = 16;

/// A star whose x drops below this respawns [`WRAP_SPAN`] to the right.
const WRAP_MIN: i32 = 0x100;
const WRAP_SPAN: i32 = 0x1200;

/// Seeding rolls: `rand(320)` columns by `rand(160)` rows.
const SEED_COLUMNS: u16 = 320;
const SEED_ROWS: u16 = 160;

/// One plane's mutable star positions plus its registry data.
struct StarPlane {
    data: StarPlaneData,
    /// `(x, y)` in 1/16-pixel units. Unseeded planes stay at the origin.
    stars: Vec<(i32, i32)>,
}

/// A level's star field: every plane's positions, advanced and drawn back to
/// front between the background and the scenery.
pub struct StarField {
    planes: Vec<StarPlane>,
}

impl StarField {
    /// Scatter the seeded planes through `rng` in the original's order: per
    /// star index, one x and one y roll for each seeded plane in turn.
    pub fn new(planes: &'static [StarPlaneData], rng: &mut EngineRng) -> Self {
        let mut planes: Vec<StarPlane> = planes
            .iter()
            .map(|&data| StarPlane {
                data,
                stars: vec![(0, 0); STARS_PER_PLANE],
            })
            .collect();

        for index in 0..STARS_PER_PLANE {
            for plane in planes.iter_mut().filter(|plane| plane.data.seeded) {
                let x = i32::from(rng.next(SEED_COLUMNS)) * SUBPIXEL;
                let y = i32::from(rng.next(SEED_ROWS)) * SUBPIXEL;
                plane.stars[index] = (x, y);
            }
        }

        Self { planes }
    }

    /// Sweep every star left by `ticks` of its plane's speed, respawning to
    /// the right past the wrap edge. The wrap check runs once per tick, as the
    /// original checks once per frame.
    pub fn advance(&mut self, ticks: u32) {
        for plane in &mut self.planes {
            let speed = plane.data.speed as i32;

            for star in &mut plane.stars {
                for _ in 0..ticks {
                    star.0 -= speed;

                    if star.0 < WRAP_MIN {
                        star.0 += WRAP_SPAN;
                    }
                }
            }
        }
    }

    /// Plot every plane's stars into the playfield rows of `frame`, back to
    /// front. `camera_y` pans the field with the rest of the playfield;
    /// `playfield_rows` clips to the rows above the panel. A plane marked
    /// `only_on_black` skips pixels the background already colored.
    pub fn render(&self, frame: &mut Framebuffer, camera_y: i32, playfield_rows: i32) {
        let width = frame.image.size.width as i32;

        for plane in &self.planes {
            for &(star_x, star_y) in &plane.stars {
                let x = star_x / SUBPIXEL;
                // The plotter writes its buffer row WITHOUT the +0x50 row
                // convention every other compositor uses, and the blit
                // never shows buffer row 0: stars land one screen row
                // higher than a direct mapping, and a star parked at row 0
                // (the races' unseeded plane B) is never drawn.
                let y = star_y / SUBPIXEL - 1 - camera_y;

                if x < 0 || x >= width || y < 0 || y >= playfield_rows {
                    continue;
                }

                let index = (y * width + x) as usize;

                if plane.data.only_on_black && frame.image.pixels[index] != 0 {
                    continue;
                }

                frame.image.pixels[index] = plane.data.color;
            }
        }
    }
}

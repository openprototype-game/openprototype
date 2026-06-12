//! Parallax scenery layers drawn over the level background.
//!
//! A level's decorative structures (Level 1's girder lattice and energy columns,
//! other levels' own formations) are not single sprites: each layer is a
//! horizontal **tilemap of catalog-cell indices**, one 32-pixel column per entry,
//! scrolled at its own rate for depth. Layers draw in a fixed order relative to
//! the playfield, so some sit behind the ship and enemies and some in front.
//!
//! This module is only the mechanism. The tilemaps themselves, the cells they
//! index, and each layer's position and speed are per-level data, decoded from
//! the level WAD and handed in via [`SceneryLayer::new`]. The mutable scroll
//! state lives separately in [`SceneryScroll`] so the layer data can stay shared
//! and immutable, the way the parallax background is split.

use crate::playfield;
use openprototype_core::framebuffer::Framebuffer;
use prototype_formats::bin::SpriteSheet;

/// One scenery column is one Mode X catalog cell wide.
const TILE_WIDTH: i32 = 32;

/// Scroll positions are 1/16-pixel fixed point, matching the parallax background,
/// so slow far layers can move at fractional speeds while keeping exact ratios.
const SUBPIXEL: u32 = 16;

/// One parallax scenery layer's data: a tilemap of catalog-cell indices, the
/// screen row it draws from, and its scroll speed. Immutable; the scroll position
/// lives in [`SceneryScroll`].
pub struct SceneryLayer {
    /// The cell to draw in each 32-pixel column, `None` for an empty column. The
    /// strip repeats as the layer scrolls.
    tiles: Vec<Option<usize>>,
    /// Screen row the layer's columns are drawn from.
    top: i32,
    /// Scroll speed, 1/16-pixel per tick.
    speed: u32,
}

impl SceneryLayer {
    pub fn new(tiles: Vec<Option<usize>>, top: i32, speed: u32) -> Self {
        Self { tiles, top, speed }
    }

    /// The strip's full length in 1/16-pixel units; restored savegame
    /// accumulators wrap here as a bounds guard.
    fn span(&self) -> u32 {
        self.tiles.len() as u32 * TILE_WIDTH as u32 * SUBPIXEL
    }

    /// The scroll position whose 10-cell window first reads the stream's
    /// terminating 0xFF jump byte: the walker zeroes the accumulator there
    /// (remainder included), so the strip loops at `len - 9` tiles and the
    /// streams' designed 9-tile head/tail overlap shows exactly once
    /// (reset re-derived at L1 file `0x9299`, L3 `0xcc51`; the idiom is in
    /// all seven WADs).
    fn reset_at(&self) -> u32 {
        self.tiles.len().saturating_sub(9) as u32 * TILE_WIDTH as u32 * SUBPIXEL
    }
}

/// A level's stack of scenery layers, in back-to-front draw order.
///
/// The faithful engine splits this around the playfield sprites: the frame
/// functions call the back layers' walkers before the ship/entity pass and the
/// front layers' after, so foreground scenery overlaps the ship (L1's row-4
/// girders, L3's fast canopy). `front_layers` counts the trailing layers on
/// the front side; the scene composites [`render_behind`](Scenery::render_behind),
/// the ship, then [`render_front`](Scenery::render_front).
pub struct Scenery {
    layers: Vec<SceneryLayer>,
    front_layers: usize,
}

impl Scenery {
    pub fn new(layers: Vec<SceneryLayer>, front_layers: usize) -> Self {
        debug_assert!(front_layers <= layers.len());

        Self {
            layers,
            front_layers,
        }
    }

    /// A fresh scroll state for these layers, all at zero.
    pub fn scroll(&self) -> SceneryScroll {
        SceneryScroll {
            offsets: vec![0; self.layers.len()],
        }
    }

    /// Place one layer's raw scroll position (a savegame's accumulator, in
    /// 1/16-pixel units), wrapped to the layer's tile span.
    pub fn restore_offset(&self, scroll: &mut SceneryScroll, layer: usize, offset: u32) {
        if let (Some(slot), Some(layer)) = (scroll.offsets.get_mut(layer), self.layers.get(layer))
            && !layer.tiles.is_empty()
        {
            *slot = offset % layer.span();
        }
    }

    /// Advance every layer's scroll by `ticks` of its own speed. A layer
    /// whose previous frame reached [`SceneryLayer::reset_at`] restarts from
    /// zero first: the original resets mid-draw, after the frame composed,
    /// so the touching frame renders un-reset and the next one starts over.
    pub fn advance(&self, scroll: &mut SceneryScroll, ticks: u32) {
        for (layer, offset) in self.layers.iter().zip(&mut scroll.offsets) {
            if layer.tiles.is_empty() {
                continue;
            }

            if *offset >= layer.reset_at() {
                *offset = 0;
            }

            *offset += layer.speed * ticks;
        }
    }

    /// Composite the layers behind the playfield sprites, in draw order,
    /// looking each column's cell up in `catalog`. Off-screen blits clip.
    /// `camera_y` is the playfield's vertical scroll, applied to every layer so
    /// the scenery pans with the background as the ship moves up and down (the
    /// original scrolls the whole playfield buffer, scenery included).
    pub fn render_behind(
        &self,
        scroll: &SceneryScroll,
        catalog: &SpriteSheet,
        frame: &mut Framebuffer,
        camera_y: i32,
    ) {
        self.render_range(scroll, catalog, frame, camera_y, false);
    }

    /// Composite the layers in front of the playfield sprites (see
    /// [`Scenery::render_behind`]).
    pub fn render_front(
        &self,
        scroll: &SceneryScroll,
        catalog: &SpriteSheet,
        frame: &mut Framebuffer,
        camera_y: i32,
    ) {
        self.render_range(scroll, catalog, frame, camera_y, true);
    }

    fn render_range(
        &self,
        scroll: &SceneryScroll,
        catalog: &SpriteSheet,
        frame: &mut Framebuffer,
        camera_y: i32,
        front: bool,
    ) {
        let split = self.layers.len() - self.front_layers;
        let range = if front {
            split..self.layers.len()
        } else {
            0..split
        };

        for (layer, &offset) in self.layers[range.clone()]
            .iter()
            .zip(&scroll.offsets[range])
        {
            render_layer(layer, offset, catalog, frame, camera_y);
        }
    }
}

/// The per-layer scroll positions for a [`Scenery`], advanced each tick. Held by
/// the scene, like the parallax offsets.
pub struct SceneryScroll {
    offsets: Vec<u32>,
}

impl SceneryScroll {
    /// One layer's raw scroll position (the savegame's accumulator view), or
    /// 0 when the layer does not exist (synthetic test assets carry none).
    pub fn offset(&self, layer: usize) -> u32 {
        self.offsets.get(layer).copied().unwrap_or(0)
    }
}

/// Blit one layer's visible columns at scroll `offset` (1/16-pixel).
fn render_layer(
    layer: &SceneryLayer,
    offset: u32,
    catalog: &SpriteSheet,
    frame: &mut Framebuffer,
    camera_y: i32,
) {
    if layer.tiles.is_empty() {
        return;
    }

    let pixel = (offset / SUBPIXEL) as i32;
    let first_column = pixel / TILE_WIDTH;
    let sub = pixel % TILE_WIDTH;
    // The original's walker draws exactly 10 cells from `si = row*80 + 4`,
    // shifted left by the sub-cell scroll: cell `k` lands at screen
    // `16 - sub + 32k`, covering the 288-wide playfield window plus bleed the
    // scene's bar mask drops.
    let visible = playfield::WIDTH / TILE_WIDTH + 1;
    let len = layer.tiles.len() as i32;

    for column in 0..visible {
        let index = (first_column + column).rem_euclid(len) as usize;

        let Some(cell) = layer.tiles[index] else {
            continue;
        };

        let Some(sprite) = catalog.sprites.get(cell) else {
            continue;
        };

        let x = playfield::LEFT + column * TILE_WIDTH - sub + sprite.origin.0;
        let y = layer.top - camera_y + sprite.origin.1;
        frame.blit_transparent(&sprite.pixels, sprite.size, x, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenery(tiles: usize, speed: u32) -> Scenery {
        let layer = SceneryLayer::new(vec![None; tiles], 0, speed);

        Scenery::new(vec![layer], 0)
    }

    #[test]
    fn the_scroll_loops_nine_tiles_before_the_stream_end() {
        let scenery = scenery(12, 512);
        let mut scroll = scenery.scroll();

        // Three advances reach (12 - 9) * 512: the 10-cell window now reads
        // the stream's jump byte, and that frame still draws un-reset.
        for _ in 0..3 {
            scenery.advance(&mut scroll, 1);
        }

        assert_eq!(scroll.offset(0), 1536);

        // The next advance restarts the strip instead of running on to the
        // decoded end.
        scenery.advance(&mut scroll, 1);
        assert_eq!(scroll.offset(0), 512);
    }

    #[test]
    fn the_reset_drops_the_subtile_remainder() {
        let scenery = scenery(12, 500);
        let mut scroll = scenery.scroll();

        // 4 x 500 = 2000 passes the 1536 threshold mid-tile; the reset
        // zeroes the whole accumulator, remainder included.
        for _ in 0..4 {
            scenery.advance(&mut scroll, 1);
        }

        assert_eq!(scroll.offset(0), 2000);

        scenery.advance(&mut scroll, 1);
        assert_eq!(scroll.offset(0), 500);
    }
}

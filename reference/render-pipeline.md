# Level render pipeline

How a level frame reaches the screen: the Mode X video setup, the 288-pixel
playfield window, the parallax background, and the order the layers compose in.
The scenery tilemap walker has its own section in
[formats/level-layout.md](formats/level-layout.md); this doc covers everything
around it.

Two address kinds appear here. **Code** is a flat file offset (`file = vaddr +
0x200`, the MZ header). **Data** is a `cs:` segment offset (`file = cs + 0x29F0`
for L1/L3/L5, `+ 0x51E0` for L7). Ground truth is
`crates/game/src/{playfield,background,scenery,stars}.rs` and
`crates/game/src/scene/level.rs`.

## Mode X 320x160

The level runs in a hand-programmed Mode X variant: 320x160 logical pixels, each
logical row scanned three times for 480 physical scanlines. The triple-scan
comes from setting the CRTC maximum-scan-line to 2 (`or $0x2` on CRTC register 9
at file `0x2d85`). A line-compare of 383 (file `0xe485`, CRTC register 0x18)
splits the display at row 128: rows 0..128 are the scrolling playfield, rows
128..160 are the fixed 32-row HUD panel (`PANEL.RAW`, 320x32). DOSBox-X reports
the live mode as `G320x160x480`.

Shown on a 4:3 CRT, 320x160 stretched to fill the frame makes each pixel 1.5x
taller than wide, a pixel aspect ratio of 1.5. The port renders into an indexed
framebuffer (palette index per pixel, expanded through the 256-color palette)
and reproduces that aspect.

## Playfield window

The scrolling layers are confined to a 288-pixel-wide window, x 16..304, with a
16-pixel black bar down each side. The panel below spans the full 320.

The window width follows from Mode X plane addressing. Each scanline is 80 bytes
(320 px / 4 planes); the level composes each row into a system buffer and blits
only bytes 4..76 of it, 72 bytes = 288 pixels, across 128 rows and 4 planes (the
`b9 12 00 f3 66 a5` blit, 18 `rep movsl` per row, in every WAD). Composition
starts at byte 4 (`si = row * 80 + 4`), so the left edge sits at x 16.

The port composes the full width and then blacks the margins
(`mask_playfield_margins`, `scene/level.rs`), which drops the same layer bleed
the original's narrow blit discards. Constants: `playfield::LEFT = 16`,
`playfield::WIDTH = 288`, `playfield::PANEL_TOP = 128`.

## Parallax background

The background is a 640-pixel-wide image (`BACKGROUND_WIDTH`) that scrolls
horizontally and wraps. It is cut into horizontal strips, each scrolling at its
own rate; the strip layout belongs to the SP image, not the level, so the three
race levels share one set. Strips are looked up by cumulative height against the
image row, and the camera pans vertically by reading each output row at
`row + camera_y`.

Horizontal position is sub-pixel. Most levels accumulate in 1/16-pixel units
(`shr 4`, wrap at `640 * 16 = 0x2800`); LEVEL_7's lava uses 1/256 units
(`shr 8`, wrap `0x28000`) for its very slow gradient creep. Each tick (one VGA
vertical refresh, ~60 Hz) advances every strip by its rate, and the offset wraps
modulo the image width.

Two per-SP constants are baked into the WAD with no code writing them before the
scroll ISR runs:

| SP set          |  initial_offset | below_strips_fill   |
| --------------- | --------------: | ------------------- |
| Canyon (L1)     |        `0x2770` | `0` (black)         |
| Alienbg (L5)    | `0xa40` (164px) | `0x5F` (dark brown) |
| Wald (L3)       |             `0` | `0` (black)         |
| Lavah (L7)      |             `0` | `0` (black)         |
| Raceb2 (L2/4/6) |             `0` | `0` (black)         |

Alienbg's `0xa40` centers its static sun strip; its `0x5F` fill repaints the
rows below the last strip every frame (the original's fill loop at file
`0xa330`). The others leave those rows black.

Per-level strip tables (strip height in pixels, scroll rate in sub-pixel units
per tick):

| Level / SP    | Strips | Heights and rates                                                           |
| ------------- | -----: | --------------------------------------------------------------------------- |
| L1 Canyon     |      7 | h = [14, 13, 9, 89, 8, 12, 15], rate = [16, 10, 6, 3, 6, 10, 16]            |
| L2/4/6 Raceb2 |      1 | h = 160, rate = 32 (the scrolling nebula band)                              |
| L3 Wald       |      1 | h = 120, rate = 4 (rows below stay black)                                   |
| L5 Alienbg    |      4 | h = [20, 23, 22, 70], rate = [8, 4, 1, 0]                                   |
| L7 Lavah      |     65 | 2-row bands, rate ramps 256 to 32 down to the horizon and back up, mirrored |

The L1 heights sum to 160, the full playfield. The race "road" is this nebula
band, not an actual road: the race levels are set in space (see
[race-mode.md](race-mode.md)).

### The scroll ISR table

The original drives the strips from a per-WAD table the timer ISR walks: a count,
then one offset/speed pair per layer. The first three entries are foreground and
HUD-overlay accumulators (offset 0); the rest are the background strips, which
the renderer reads from the same storage. The per-WAD bases and counts:

| WAD     | ISR table   | Layers |
| ------- | ----------- | -----: |
| LEVEL_1 | `cs:0x25F2` |     10 |
| LEVEL_2 | `cs:0x2804` |      4 |
| LEVEL_3 | `cs:0x38CC` |      5 |
| LEVEL_4 | `cs:0x287C` |      4 |
| LEVEL_5 | `cs:0x25F7` |      7 |
| LEVEL_6 | `cs:0x2D7C` |      4 |
| LEVEL_7 | `cs:0x2C58` |     68 |

The port models the offsets as a flat accumulator list with modulo wrap rather
than reproducing the ISR add-loop. The accumulators are the same values a save
carries; their save order is in [savegame.md](savegame.md).

## Compose order

`scene/level.rs` builds each frame back to front:

1. Parallax background (rows 0..128).
2. Star field.
3. Scenery behind the ship (`render_behind`).
4. Enemy and pickup spawns (the display list).
5. Weapon orbs and overlay (skipped while the ship is dying).
6. Ship and shield, then the muzzle flash. While dying, the ship explosion
   replaces this whole block (gate `cs:0x46b2`).
7. Scenery in front of the ship (`render_front`).
8. Black the playfield margins.
9. Dim the playfield if frozen (in-game menu or GET READY), rows 0..128 only.
10. The weapon-top overlay (panel header, selected weapon, the slide animation).
11. The HUD panel and the weapon pod (see [hud.md](hud.md)).
12. The in-game menu over the dimmed playfield, or the GET READY text.

The freeze dim runs after every playfield layer but before the overlay and
panel, so the rows above the panel stay bright while the play area darkens. The
star field draws between background and scenery; the front scenery layers (the
per-level `front_layers` count) draw after the ship.

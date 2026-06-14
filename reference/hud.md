# HUD: the status panel

The 32-row panel below the playfield (rows 128..160), drawn from a set of `.RAW`
sheets blitted into the Mode X panel band. It shows the score, lives, the four
weapon-charge bars, the smart-bomb count, the weapon selector lights, and the
animated weapon pod. Ground truth is `crates/game/src/hud.rs` and
`crates/game/src/assets.rs`.

Addresses are flat file offsets (`file = vaddr + 0x200`) for code and `cs:`
offsets for data. The panel band is the Mode X region described in
[render-pipeline.md](render-pipeline.md).

## Screen mapping

Every element places itself with a `di` offset into the panel's plane memory at
an 80-byte stride (320 px / 4 planes). A `di` maps to a pixel as
`x = (di % 80) * 4`, `y = panel_top + di / 80`, with `panel_top = 128`. The port
keeps the same `di` constants so the layout matches byte for byte.

## Elements

`draw_hud` composes the panel in this order: background, score, lives, weapon
bars, smart bombs, selector. The pod is drawn separately by the scene.

| Element         | `di`                   | Asset       | Sheet   | Cell / window | Routine      |
| --------------- | ---------------------- | ----------- | ------- | ------------- | ------------ |
| Panel           | `0`                    | PANEL.RAW   | 320x32  | full          | blit         |
| Score           | `0x325`, +4/digit      | SCORE.RAW   | 16x130  | 16x13 digit   | `glyph`      |
| Lives           | `0x34C`                | NUMBERS.RAW | 12x90   | 12x10 digit   | `glyph`      |
| Weapon bars     | `0x172`, pitch `0x230` | BALKEN.RAW  | 64x16   | 32x4 window   | `bar_window` |
| Smart bombs     | `0x744`                | SMART.RAW   | 40x36   | 40x9 frame    | `glyph`      |
| Selector lights | `0x12B`, pitch `0x230` | LIGHTS.RAW  | 12x56   | 12x7 glyph    | `glyph`      |
| Weapon pod      | `0x3F` (x 252)         | EXTRAS.RAW  | 280x192 | 56x32 cell    | `pod_cell`   |

Element specifics:

- **Score** draws six digits most-significant first, advancing `di` by 4 (16 px)
  each, with leading zeros. The original extracts the digits by six divide-by-10
  remainders into `cs:0x2683`.
- **Lives** draws a single digit of `lives - 1`, nothing at zero (the death path
  exits before a zero would draw).
- **Weapon bars** pick a source column from the 64-wide sheet at `level * 8`,
  clamped to 31, giving the 0..4 fill as a 32x4 window per weapon.
- **Smart bombs** show frame `min(count, 3)`.
- **Selector lights** are a 4-slot strip; the selected slot
  (`cs:0xcb7`) draws its highlighted glyph (`4 + slot`) instead of the base one.

## Glyph blitter

The shared glyph blitter (`cs:0xe109`) copies a stacked-sheet cell: source
`si = 0xdf20 + glyph * 0x34`, 4 bytes per row across 13 rows, advancing the
destination by `0x4c` per row. The port's `glyph` helper is the same idea: it
slices `index` contiguous rows out of a single-column sheet, and backs the score
digits, lives, smart-bomb frames, and selector lights. The bars and the pod use
2D slicers (`bar_window`, `pod_cell`) because their sheets are not single
columns.

## Assets

The loader fills a segment-slot table at `cs:0x281f`, two bytes per slot. The
port loads the same eight sheets in `load_hud_assets`. Sizes are the raw 8-bit
linear byte counts.

| Slot        | Asset       | Dimensions | Bytes |
| ----------- | ----------- | ---------- | ----: |
| `cs:0x2829` | LIGHTS.RAW  | 12x56      |   672 |
| `cs:0x282B` | BALKEN.RAW  | 64x16      |  1024 |
| `cs:0x282D` | EXTRAS.RAW  | 280x192    | 53760 |
| `cs:0x282F` | PANEL.RAW   | 320x32     | 10240 |
| `cs:0x2831` | SCORE.RAW   | 16x130     |  2080 |
| `cs:0x2833` | FONT.RAW    | (overlay)  |     — |
| `cs:0x2835` | NUMBERS.RAW | 12x90      |  1080 |
| `cs:0x2837` | SMART.RAW   | 40x36      |  1440 |

FONT.RAW has a slot but the HUD loader does not use it; the font is decoded
separately for the overlay and intro text. The panel palette is not a sheet: it
is the level's embedded palette read from the WAD (see
[formats/wad.md](formats/wad.md)).

## Redraw cadence

The original draws the whole panel once per level (file `0xe034`, called from
`0xf733`) and then updates only two things per frame: the score (file `0xe120`,
redrawing just the digits that changed against a per-digit cache at `cs:0x2689`)
and the pod animation (file `0xab4a`). The port recomposites the entire panel
every frame, which is output-equivalent.

## Weapon pod

The pod cell is picked from the EXTRAS sheet by column (weapon) and row (animation
frame), at `di = 0x3F` (x 252). The original animates a weapon change as a
two-phase state machine: it lowers the old pod, then raises the new one, stepping
the frame every 6th tick (flag `cs:0x2697`, phase `cs:0x2698`, frame
`cs:0x2699`, latched weapon `cs:0x2695`). The port currently raises the resolved
pod directly from the scene's latch without the lower-old-pod phase.

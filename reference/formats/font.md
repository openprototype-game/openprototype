# FONT.RAW / FONT2.RAW: bitmap fonts

Status: verified by rendering the original main menu.

## Layout

A font is an uncompressed 256-color glyph sheet, fixed 320 px wide. Height
comes from the byte count (`FONT.RAW` and `FONT2.RAW` are both 19840 bytes =
320 x 62).

- Glyphs are 16 px wide, 15 px tall, laid out 20 per row.
- The glyph area starts at `y = 16`; the top 16 rows are not glyph data.
- Character `c` maps to glyph index `c - 0x20` (space is the first glyph). The
  sheet is uppercase-only.
- Pixel index 0 is transparent, so text composites over a background instead of
  painting a box.

For glyph `n`: `band = n / 20`, `column = n % 20`. The glyph's top-left source
pixel is at `x = column * 16`, `y = 16 + band * 15`. Copy 15 rows of 16 px,
skipping index-0 pixels, then advance 16 px to the next character cell.

## Evidence

Reverse-engineered from START.EXE's glyph blitter `fcn.00003d03`: source base
`si = 0x1400` (= 320 * 16, the y = 16 start), stride 0x12c0 (= 320 * 15) per row
of 20 glyphs, `+ n * 16` within the row, 15 scanlines copied, index 0 left as
background. See `reference/start-exe.md`.

Rendering `BACK3.RAW` (320x200 menu background) with `FONT.RAW` text on top,
using the menu palette in START.EXE (see below), reproduces the original main
menu exactly: gray background, gray-to-rust labels, and the orange triangle
cursor. Glyph `0x3e` (drawn for `'>'`) is that triangle, not an ASCII `>`.

## Menu palette

The menu palette is not a `.PAL` file. It lives inside START.EXE at image offset
`0x5130` (file offset **21296**; 768 bytes, 6-bit VGA: index 0 black, index 1
white, then a gray ramp). The `start_exe` decoder reads it via `StartExe`.
`entry0` copies it from segment `0x513:0` into a buffer and uploads 256 colors
through `fcn.00000230` (DAC ports 0x3c8/0x3c9) just before the menu loop; see
`reference/start-exe.md`. Getting the offset wrong by a few bytes rotates the
RGB channels and tints everything green, which is how the right alignment was
found.

## Open

- `FONT2.RAW` has the same dimensions; its distinct glyph set (different size or
  color for the HUD) is not yet rendered.

# WAD: level files

`LEVEL_1.WAD` .. `LEVEL_7.WAD`, one per level. Each is a DOS MZ executable: the
level's program with its palette and data compiled in. Gameplay assets (sprites,
audio) are loaded from separate disc files, not packed inside.

This doc covers the container: the MZ image, the embedded palette, and the asset
filename strings. How a level's enemy/pickup objects are placed (the generated
scatter and the race levels' static table) is in `level-layout.md`.

## Container: it is a plain DOS MZ EXE

Every WAD is a valid MZ executable, and nothing is appended after the load image: the
header's declared image size equals the file size for all seven, so there is no
overlay or trailing blob. The palette and the level data live *inside* the loaded
image (the program's data, read by its own code), not in a tacked-on section. The MZ
header is 512 bytes (32 paragraphs); code starts at file offset 512.

| WAD     | File size | Relocs | cs:ip       | Palette @ |
|---------|-----------|--------|-------------|-----------|
| LEVEL_1 | 75472     | 64     | 027F:CCFE   | 0x26F0    |
| LEVEL_2 | 61328     | 53     | 007B:B2A1   | 0x06B0    |
| LEVEL_3 | 89952     | 91     | 0451:EAB1   | 0x4410    |
| LEVEL_4 | 61552     | 53     | 007B:B319   | 0x06B0    |
| LEVEL_5 | 81840     | 81     | 03D9:CB83   | 0x3C90    |
| LEVEL_6 | 64112     | 53     | 007B:B819   | 0x06B0    |
| LEVEL_7 | 92272     | 81     | 0451 (0x4FE):DD50 | 0x4EE0 |

LEVEL_2, 4, 6 share an identical header shape (53 relocs, cs=007B) and the same
palette offset. Those are the three RACEB2 race levels, built from one common code
base; the others are each their own build.

## Palette: locate by signature, not by offset

Each WAD embeds the 256-color palette as a raw 768-byte block of 6-bit VGA values
(the same layout as a `.PAL`). The offset differs per WAD (see the table) and follows
no fixed rule, so it cannot be hardcoded or computed.

It is reliably found by signature instead. The block opens with black then white then
a 30-step descending gray ramp, so the first 32 entries are grayscale. The locator:
scan for `00 00 00 3F 3F 3F` followed by a 768-byte window whose every byte is
`<= 0x3F`. Across all seven WADs this matches in exactly one place each, so the
signature is unambiguous. (It can be tightened further by also requiring the full
32-entry gray ramp, if a false positive ever turns up.)

## One master palette, two variants

The seven palettes are not seven different ones:

- LEVEL_1, 2, 4, 5, 6 carry a byte-identical **master palette**.
- LEVEL_3 (WALD) is the master palette with only its bottom row changed (foliage
  greens).
- LEVEL_7 (LAVAH) is a more divergent variant (lava oranges and reds).

So the game authored most backgrounds against one shared 256-color palette and
swapped in tweaked variants only where a level needed colors the master lacked.

Verified by rendering each background with the palette found in its WAD:

- CANYON (master): correct. Tan canyon walls, dark sky, pylons.
- ALIENBG (master): correct. Coherent hazy sky with a bright sun.
- LAVAH (own variant): correct. Glowing orange and red lava.
- WALD (own variant): renders mostly dark with a green-speckled band at the top. This
  is correct: the palette is right (master plus the green bottom row), and the dark
  background is by design. The level's vivid color comes from the foreground trees,
  which are BIN sprites composited on top, not from the background.

## Asset references

The asset filenames the level loads from disc appear as packed, null-terminated ASCII
strings in one region of the WAD (in LEVEL_1 at 0x5137: `out.bin`, `pturn1.bn1`,
`canyon.sp1`..`canyon.sp4`, then the HUD RAWs). They are arguments the level code
passes to DOS file open, not an index into the WAD. The exact record layout
(fixed-width slots vs. plain packed strings) is still open.

### Level to assets mapping

| WAD       | Background | Other        |
|-----------|------------|--------------|
| LEVEL_1   | CANYON     | out.bin      |
| LEVEL_2   | RACEB2     | race1.bin    |
| LEVEL_3   | WALD       | wald.bin     |
| LEVEL_4   | RACEB2     | race1.bin    |
| LEVEL_5   | ALIENBG    | techno.bin   |
| LEVEL_6   | RACEB2     | race1.bin    |
| LEVEL_7   | LAVAH      | lava.bin     |

Every WAD also references the shared HUD/UI RAW files: `panel.raw`, `score.raw`,
`numbers.raw`, `lights.raw`, `smart.raw`, `extras.raw`, `balken.raw`, `font.raw`,
`0.raw`.

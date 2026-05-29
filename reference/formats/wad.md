# WAD: level files

Status: partly investigated. Structure not yet decoded.

## What's confirmed

A `.WAD` is a DOS EXE that acts as a file index for one level: it lists the
asset filenames it loads externally (verified via ASCII strings), e.g.
`canyon.sp1`, `panel.raw`, `out.bin`. So assets are referenced, not compiled in.

Each WAD embeds the level's 256-colour palette as a raw 768-byte block of 6-bit
VGA values (the same layout as a `.PAL`). For `LEVEL_1.WAD` it sits at offset
**9968 (0x26F0)** and opens with black, white, then a descending gray ramp.
Applying it to the CANYON background produces correct colours (tan canyon
walls, dark sky, pylons). That confirms both the SP decode and the palette.

The palette offset is **per-WAD**, not fixed: each level WAD places its 6-bit
run at a different offset. Locating it reliably needs the WAD's file-index
format, not a hardcoded offset.

## Level to assets mapping

From the filename strings in each WAD:

| WAD       | Background | Other        |
|-----------|------------|--------------|
| LEVEL_1   | CANYON     | out.bin      |
| LEVEL_2   | RACEB2     | race1.bin    |
| LEVEL_3   | WALD       | wald.bin     |
| LEVEL_4   | RACEB2     | race1.bin    |
| LEVEL_5   | ALIENBG    | techno.bin   |
| LEVEL_6   | RACEB2     | race1.bin    |
| LEVEL_7   | LAVAH      | lava.bin     |

Every WAD also references the shared HUD/UI RAW files: `panel.raw`,
`score.raw`, `numbers.raw`, `lights.raw`, `smart.raw`, `extras.raw`,
`balken.raw`, `font.raw`, `0.raw`.

## Open

- The file-index format (how filenames map to offsets/sizes, and where the
  palette entry lives in it). This is the level-structure decode.
- Whether the `.BIN` per-level file (out.bin, etc.) holds the level layout,
  enemy spawns, or sprite data.

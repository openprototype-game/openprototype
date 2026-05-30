# BIN / BN1: compiled sprites

Status: rendering path reverse-engineered (in r2, from `LEVEL_1.WAD`) and
verified by rendering — the OUT.BIN rock/asteroid reproduces the game screenshot
pixel-for-pixel.

The per-level `.BIN` files (and the shared `PTURN1.BN1`) hold **compiled
sprites**: short x86 subroutines that write palette indices straight into VGA
Mode X memory. A level WAD loads its BIN, then `CALL FAR`s into these
subroutines to draw each sprite. This is the classic DOS "compiled sprite"
fast-blit technique.

## Subroutine format

Each sprite plane is one subroutine, terminated by `RETF` (`0xCB`). The only
opcodes used (all writing relative to `SI`, the destination pointer):

| bytes | instruction | meaning |
|-------|-------------|---------|
| `C6 44 dd vv` | `mov byte [si+disp8], imm8` | write 1 pixel |
| `C7 44 dd vvvv` | `mov word [si+disp8], imm16` | write 2 pixels |
| `C6/C7 84 dddd ..` | `[si+disp16]` form | disp is 16-bit |
| `66 C7 .. vvvvvvvv` | `mov dword [si+disp], imm32` | write 4 pixels |
| `81 C6 vvvv` | `add si, imm16` | advance destination |
| `03 F0` | `add si, ax` | advance one row (AX = stride) |
| `B8 vvvv` | `mov ax, imm16` | set stride |
| `CB` | `retf` | end of subroutine |

Only the non-transparent pixels are emitted, so unwritten bytes stay transparent.

## Mode X drawing convention

The level engine runs VGA **Mode X** (unchained planar, 320x200): BIOS mode 13h
then sequencer reg 4 = `0x06` to unchain (`LEVEL_1.WAD` @ `0x2942`). Video is at
`ES = 0xA000`. The executor (`@0x803f`) calls a subroutine with **`AX = 0x50`
(80)** = bytes per plane-row.

A full sprite is **4 subroutines, one per plane**. The dispatcher (`@0x87ba`)
sets the Map Mask (sequencer reg 2) to `0x01/0x02/0x04/0x08` in turn and calls
the matching plane subroutine. Plane `p` owns the screen columns where
`x % 4 == p`, so a plane-buffer offset `O` maps to:

```
x = (O % 80) * 4 + p
y =  O / 80
```

Four dispatcher variants (`0x87ba/0x8826/0x8894/0x8902`) rotate the plane order
and `inc si` to handle the four sub-pixel X alignments (`x % 4`).

## Per-sprite descriptor table (in the WAD, not the BIN)

The association of 4 plane-subroutines into one sprite lives in a table in the
**WAD**, not the BIN — nothing in the BIN groups them. For `LEVEL_1.WAD` it is
in segment `0x0F7F` (file offset `0xf9f0`). Records are **10 bytes = 5 words**:

```
[ field0 | p0 | p1 | p2 | p3 ]
```

`p0..p3` are byte offsets into the BIN, one per plane. Each `pk` points to that
plane's **progressive-clip header** (a 5-word table at the start of each
sub-sprite, e.g. OUT.BIN @ 0 = `0a 00 0f 00 18 00 21 00 2c 00`). The actual
subroutine for an unclipped sprite is:

```
sub_offset = pk + u16(BIN[pk])      // variant 0 (full sprite)
```

Variants 1-4 are progressively left-clipped versions sharing the trailing
`RETF`; the engine selects one via a clip offset (`@0x7c64` picks it from screen-edge
proximity; the base is stored at `cs:0x59ae`).

The draw loop (`@0x8ee2`) walks a row of sprites: `bx += 0x0a` (next descriptor),
`bp += 0x20` (X += 32 — sprites are 32px wide), clipping at X in `[-32, 288]`.

## Level → BIN mapping

Per the filename strings (see [wad.md](wad.md)): LEVEL_1 → OUT.BIN, LEVEL_3 →
WALD.BIN, LEVEL_5 → TECHNO.BIN, LEVEL_7 → LAVA.BIN, LEVEL_2/4/6 → RACE1.BIN; all
levels also load the shared `PTURN1.BN1` (player-ship sprites).

## Verified vs open

- **Verified**: dense ~32x32 OUT.BIN sprites render correctly through the full
  chain — the floating rock/asteroid is pixel-accurate. This proves the
  compiled-sprite format, the Mode X convention, and the descriptor-table
  grouping.
- **NOT yet right**: many descriptor records (the tall / sparse ones) still
  render as garbage with the current variant-0, no-clip assumptions. So the
  descriptor reading is not yet general — some records are not plain 32x32
  sprites (record type via `field0`? clip variant? a different draw path?). Only
  the dense sprites are confirmed.
- **Open**: `PTURN1.BN1` (the player ship) is drawn by an analogous but separate
  path with its own descriptor table, not yet traced.

Addresses are r2 flat vaddr for `LEVEL_1.WAD`; file offset = vaddr + `0x200`
(the MZ header).

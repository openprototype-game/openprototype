# BIN / BN1: compiled sprites

Status: rendering path reverse-engineered (in r2, from `LEVEL_1.WAD`) and
verified by rendering — OUT.BIN banks 0–6 decode into recognizable sprites
(rotating asteroid, blue-lit metal girders, explosions). 1984/2056 plane
subroutines (96.5%, banks 0–38 fully) decode through the full chain.

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

The draw loop (`@0x8ee2`) reads `field0` into `bp`, then the dispatcher
(`@0x87ba`) reads the plane words from `gs:[bx+2/4/6/8]`. Each iteration:
`bx += 0x0a` (next descriptor), `bp += 0x20` (X += 32 — sprites are 32px wide),
clipping at X in `[-32, 288]`.

### `field0` is an EMS logical page

OUT.BIN is 2 MB, too big for conventional memory, so the level loads it into
**expanded memory (EMS)**. The loader (`@0x7da1`) allocates 160 16 KB pages
(`AH=43h, BX=0xA0`) and reads the file in sequential 16 KB chunks, one per
logical page (`mov cx,0x4000; int 21h`). The fill is identity: file bytes
`[i*0x4000 .. (i+1)*0x4000)` = EMS logical page `i`.

`field0` is that logical-page index. Before drawing, the clip-select routine
(`@0x7c64`) maps logical page `field0` into the page frame (`AX=4400h..4403h,
DX=field0, int 67h`) and stores the frame segment at `cs:0x5856` → `fs`. So a
record's **bank base in the file is `field0 * 0x4000`**, and `p0..p3` are offsets
relative to that base.

### Two kinds of plane pointer

Each plane word `pk` resolves to a subroutine one of two ways:

- **Clip-header (small, clippable sprites):** `pk` points at a 5-word
  progressive-clip header (e.g. OUT.BIN @ 0 = `0a 00 0f 00 18 00 23 00 2e 00`).
  The dispatcher computes `sub = pk + u16(fs:[pk + clipbase])`, where
  `clipbase = cs:0x59ae` ∈ {0,2,4,6,8} selects the variant (0 = unclipped).
  Variants 1-4 are progressively left-clipped, sharing the trailing `RETF`;
  `@0x7c64` picks `clipbase` from screen-edge proximity.
- **Direct (large/tall sprites, e.g. the 32×145 girders):** `pk` points straight
  at the subroutine — no header, no clipping. `sub = pk`.

Decoder discriminator: if `BIN[base+pk]` is a draw opcode
(`C6/C7/66/B8/81/03/CB`), the pointer is direct; otherwise it's a clip header.

## Level → BIN mapping

Per the filename strings (see [wad.md](wad.md)): LEVEL_1 → OUT.BIN, LEVEL_3 →
WALD.BIN, LEVEL_5 → TECHNO.BIN, LEVEL_7 → LAVA.BIN, LEVEL_2/4/6 → RACE1.BIN; all
levels also load the shared `PTURN1.BN1` (player-ship sprites).

## Verified vs open

- **Verified**: OUT.BIN banks 0–6 render into recognizable sprites — rotating
  asteroid, blue-lit metal girders (the 32×145 direct-pointer sprites that used
  to decode as garbage), explosions. Across the whole table, 1984/2056 plane
  subroutines (banks 0–38 fully) decode through the chain. This proves the
  compiled-sprite format, the Mode X convention, the EMS-page banking, and both
  pointer kinds.
- **Open**: the descriptor table's true record count. ~496+ decode as garbage,
  which is most likely past the table's real end (the draw loop's `cx` count,
  not yet read), not a format gap. Pixel/colour fidelity is eyeballed, not
  diffed against captures.
- **Open**: `PTURN1.BN1` (the player ship) is drawn by an analogous but separate
  path with its own descriptor table, not yet traced.

Addresses are r2 flat vaddr for `LEVEL_1.WAD`; file offset = vaddr + `0x200`
(the MZ header).

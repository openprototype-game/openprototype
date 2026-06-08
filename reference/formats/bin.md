# BIN / BN1: compiled sprites

Status: rendering path reverse-engineered (in r2, from `LEVEL_1.WAD`) and
verified by rendering — OUT.BIN decodes into recognizable sprites (rotating
asteroid, blue-lit metal girders, explosions). All 4112 plane subroutines of the
1028 catalog records resolve (100%).

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

A full sprite is **4 subroutines, one per plane**. A dispatcher sets the Map
Mask (sequencer reg 2) to `0x01/0x02/0x04/0x08` in turn and calls the matching
plane subroutine via the executor (`@0x803f`). Plane `p` owns the screen columns
where `x % 4 == p`, so a plane-buffer offset `O` maps to:

```
x = (O % 80) * 4 + p
y =  O / 80
```

The header-family dispatcher has four variants (`0x87ba/0x8826/0x8894/0x8902`)
that rotate the plane order and `inc si` to handle the four sub-pixel X
alignments (`x % 4`).

## Per-sprite descriptor table (in the WAD, not the BIN)

The association of 4 plane-subroutines into one sprite lives in a table in the
**WAD**, not the BIN — nothing in the BIN groups them. For `LEVEL_1.WAD` it is
in segment `0x0F7F` (file offset `0xf9f0`). Records are **10 bytes = 5 words**:

```
[ field0 | p0 | p1 | p2 | p3 ]
```

The table is a shared sprite **catalog**: **1028 records** (`0xf9f0 .. 0x12218`),
referenced by index from the level's object code. A header-family draw
loop (`@0x8ee2`) reads `field0` into `bp`, then its dispatcher (`@0x87ba`) reads
the plane words from `gs:[bx+2/4/6/8]`. Each iteration: `bx += 0x0a` (next
descriptor), `bp += 0x20` (X += 32 — sprites are 32px wide), clipping at X in
`[-32, 288]`.

The WAD tail after the catalog: 8 zero bytes (`0x12218`), then **three 16-colour
VGA-6bit ramps** (`0x12220 .. 0x122b0`, lighting/fade LUTs, 48 bytes each, the
2nd and 3rd identical), then zero padding to the WAD end (`0x126d0`).

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

### Two kinds of plane pointer (two draw families)

A plane word `pk` resolves to a subroutine one of two ways, matching the two
dispatcher families in the engine:

- **Clip-header family** (dispatcher `@0x87ba`): for small clippable game objects
  (rocks, particles, explosions). `pk` points at a progressive-clip header — a
  short word table (e.g. OUT.BIN @ 0 = `0a 00 0f 00 18 00 23 00 2e 00`; some
  sprites have 3 words, not 5). The dispatcher computes
  `sub = pk + u16(fs:[pk + clipbase])`, where `clipbase = cs:0x59ae` ∈ {0,2,4,6,8}
  selects the variant (0 = unclipped). The variants are progressively
  left-clipped, sharing the trailing `RETF`; `@0x7c64` picks `clipbase` from
  screen-edge proximity.
- **Direct family** (dispatcher `@0x804a` + variants): for large full-height
  objects (e.g. the 32×145 girders). `mov cs:[0x5854], gs:[bx]` — the plane word
  *is* the subroutine offset, no header, no clipping. `sub = pk`. This family
  advances `si` by a full plane-screen (`0x4000`) per plane.

Which family draws a given catalog entry is decided by the referencing code, but
the entry's own data is self-describing, so a decoder needs no caller context:
read `word0 = u16(BIN[base+pk])`; if `word0` is a small offset (1..0x7ff) and
`base+pk+word0` decodes to a valid `RETF`-terminated subroutine, it's a clip
header (use `sub = pk + word0`); otherwise `pk` is a direct subroutine. This is
unambiguous across all 1028 catalog records (4112/4112 planes resolve). Do **not**
discriminate on the first byte alone — a header whose `word0` is e.g. `0x0066`
starts with byte `0x66`, which collides with the operand-size-prefix opcode.

## Level → BIN mapping

Per the filename strings (see [wad.md](wad.md)): LEVEL_1 → OUT.BIN, LEVEL_3 →
WALD.BIN, LEVEL_5 → TECHNO.BIN, LEVEL_7 → LAVA.BIN, LEVEL_2/4/6 → RACE1.BIN; all
levels also load the shared `PTURN1.BN1` (player-ship sprites).

## PTURN1.BN1 (player ship)

`PTURN1.BN1` is 44,881 bytes = **232 compiled-sprite subroutines** back-to-back,
no internal catalog. It fits one segment, so there's no EMS banking (no
`field0`). The ship sprites are all **direct** (raw subroutine pointers, no clip
header — the ship is never edge-clipped).

The grouping is a catalog in the WAD at vaddr `0x6bdc` (file `0x6ddc`): a flat
array of **8-byte cell records `[p0, p1, p2, p3]`** (4 plane offsets into PTURN1),
delimited into frames by a trailing `ncells` word. The draw loop (`@0x8b80`,
four X-align variants) is handed a frame's first cell and `cx = ncells`; it walks
8-byte cells, calling the direct dispatcher (`@0x804a`) per cell and advancing
X += 32 per cell. So a frame is `ncells` cells of 64/96px width.

**28 ship frames render correctly** (rotating/banking craft with twin green
exhausts), covering 228 of the 232 subroutines.

## Verified vs open

- **Verified**: every one of the 1028 catalog records decodes — all 4112 plane
  subroutines resolve through the chain (100%). Rendered sprites are recognizable
  (rotating asteroid, blue-lit girders, explosions). This is code-grounded: the
  opcodes, Mode X convention, EMS-page banking, and both draw families come from
  the disassembly, and the per-entry header/direct rule is a self-describing
  property of the data, not a guess.
- **Open**: pixel/colour fidelity is eyeballed, not diffed against real captures.
- **Open**: the runtime placement layer — *which* catalog entry is drawn *where*
  and *when* (scroll `cs:0x6404`, the per-object tables) — is needed for a
  faithful live renderer but not for extracting the sprites. Not traced.
- **Open (PTURN1 tail)**: 4 of the 232 PTURN1 subroutines aren't accounted for —
  grouping the last 8 subs as a 2-cell frame renders an incoherent strip, so they
  don't form a standalone ship frame. They're reached (if at all) through a
  separate sprite-directory layer: tables of `[X, ncells, w, h]` / `[w, h, X,
  ncells]` records near the catalog (bounding boxes `51×33`, `94×51`, and tiny
  `4×4`), where `X = 473, 475, … 511, 514 …` indexes a pool **larger than
  PTURN1's 58 cells** — i.e. a cross-sprite/global directory the level engine
  maintains, not part of the ship file itself. **There is at least one sprite in
  the PTURN1 tail not yet identified.** Reverse-engineering that directory is a
  separate, larger task (the general object catalog), out of scope for the sprite
  encoding.

Addresses are r2 flat vaddr for `LEVEL_1.WAD`; file offset = vaddr + `0x200`
(the MZ header).

## The Mode X sprite blitter (the placement-layer renderer)

A sprite-drawing routine (LEVEL_2 vaddr `0x6744`..`0x6829`, a family of
near-identical loops) is the Mode X blitter that draws the catalog. It walks the BIN
sprite catalog, not the level's object-layout table (its loop cursor is screen
memory). This is the rendering side of the layout system in `level-layout.md`.

- The loop's `si` is a VRAM destination cursor: setup computes `si = row * 80 + x`
  (Mode X is 80 bytes per row), and `si += 8` per step moves 32 pixels right. The
  `add si, 8` is screen columns, not a data-record stride.
- Objects are drawn on a 32-pixel horizontal grid with on-screen culling (x between
  -32 and 288).
- The per-object sprite data is a 10-byte array walked by `bx` (`bx += 0xA`, base from
  a control struct via `cs:[di+6]`): a word EMS page id followed by four words that
  are compiled-sprite plane entry points. This is exactly the **BIN sprite catalog**
  decoded above (`decode_banked`, e.g. `OUT_BIN_CATALOG`).
- The sprites are compiled code: the blitter patches a `0xCB` (`retf`) into the
  sprite, calls into it, then restores the byte. Four near-identical loop variants are
  selected by a jump table on `x & 3`, the four Mode X pixel phases.
- Sprite pages are banked through EMS: the helper at vaddr `0x56EA` maps a page with
  `int 0x67` (`ax = 0x4400`) behind a 4-slot LRU cache (`cs:[0x4F32..]`).

For the Rust port none of the EMS banking or compiled-sprite machinery carries over
(no 640 KB limit); what matters is the data it ultimately places. Addresses in this
section are LEVEL_2 vaddr.

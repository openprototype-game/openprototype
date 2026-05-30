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
referenced by index from the level's object/scenery code. A header-family draw
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
  scenery (e.g. the 32×145 girders). `mov cs:[0x5854], gs:[bx]` — the plane word
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
- **Open**: `PTURN1.BN1` (the player ship) is drawn by an analogous but separate
  path with its own catalog, not yet traced.

Addresses are r2 flat vaddr for `LEVEL_1.WAD`; file offset = vaddr + `0x200`
(the MZ header).

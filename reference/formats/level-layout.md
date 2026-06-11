# Level object layout

How each level decides where its enemy and pickup objects spawn. Two
mechanisms, one record format:

- **Race levels (2, 4, 6)** bake a static placement table into the EXE (`0x1690`).
- **Generated levels (1, 3, 5, 7)** build the placement at load from a PRNG-driven
  layout script, so the scatter varies every play. All four are fully decoded,
  ported, and validated byte-for-byte against the running game.

The objects placed here are **enemies and pickups**, confirmed by rendering
LEVEL_1's 12 distinct sprite types in the level palette. A level's decorative
scenery (LEVEL_1's girder lattice and columns) is a third, separate mechanism,
covered under [Scenery layers](#scenery-layers): parallax tilemap layers of
catalog cells.

Both kinds emit the same 8-byte object record. The WAD container itself (MZ image,
palette, asset strings) is in `wad.md`; this doc is only the layout system.

All four run through one interpreter (`slot.rs`); the per-level data is in
`level_<n>.rs`.

## The object record

8 bytes, 4 words: `{word0 = x-step, word1 = sprite ptr, word2 = depth, word3 = y}`.
Authoritative from the generator's write trace (`cs:[bp+0..6]`).

- **word0 (x-step)** is a horizontal *step*, not an absolute x. The consumer
  running-sums the steps to the absolute x along the scroll (CANYON totals
  ~11,800 px, ~37 screens).
- **word1 (sprite ptr)** is a pointer into a descriptor region. The target is a
  4-word descriptor `(0x0008, 0x7d00, 0x3308, 0x000a)` whose second word `0x7d00`
  is a sprite-data / OUT.BIN offset. Some descriptors (`0x392e`, `0x3f8e`) point at
  other `0x3308`-style values (nested). Pointer-vs-index resolution is a render-side
  detail to pin when wiring sprites.
- **word2 (depth)** is a per-sprite-type draw layer (z-order among the objects),
  fixed per sprite type. It picks the layer the object draws in; it does *not* lock
  the object to a background strip's scroll speed (a spawned enemy moves under its
  own AI).
- **word3 (y)** is the vertical position.

The same 8-byte shape appears in the race levels' static `0x1690` table, so both
level kinds share one object-record format.

Runtime buffer base: the records start at the C-runtime marker + `0x48`. Read there,
the runtime records are exactly `{x, sprite, depth, y}`, matching the write trace.

## Level architecture (parallax strips)

The level's background and parallax architecture:

- `SP1`..`SP4` are the **four Mode X planes of one combined background image**. That
  image is **split into horizontal strips**, each scrolling at a **different speed**
  for the parallax depth illusion. The SP planes are all background; the foreground
  girders that draw in front of the ship are a scenery layer (see
  [Scenery layers](#scenery-layers)).
- The combined image is **taller than the screen**: flying up/down scrolls it
  vertically, so the full height is not always visible. The strip **seams** show in
  the Stage 1 combined render (`re/canyon_*`), where it gets cut.
- The **background** strips are the SP planes; their scroll is the parallax. The
  placed **objects** (the generated buffer at vaddr `0x34dc`) are enemies and
  pickups, not background scenery. They do not ride the strips: `word2` (depth) is a
  draw layer, and a spawned enemy moves under its own AI. The per-frame display list
  the blitters read is built at runtime from this buffer as the scroll brings each
  spawn-x into view.
- The non-race levels **generate** their spawn placement at load (PRNG scatter,
  reseeded from the wall clock, so the layout varies every play); the race levels
  bake an exact table. Randomised placement suits enemy waves; the race obstacle
  courses need it fixed.

Data-strategy note: decoded design data (layout scripts, depth tables) is the
recreated game's own content, not the creative assets (sprite pixels, audio,
palettes) we read from the original disc. For the generated levels there is nothing to
extract anyway: we port the generator and its small per-layer config.

## Scenery layers

LEVEL_1's girder lattice and energy columns are drawn as parallax tilemap layers, a
third placement mechanism separate from the enemy/pickup object table and the SP
background. Each layer is a horizontal map of catalog-cell codes, one 32-pixel
column per byte, scrolled at its own rate and composited in a fixed pass relative to
the playfield.

### The tilemap

The renderer (`0x9237`) points `cs:0x31c4` at a layer's tilemap and reads the tile
byte at `cs:0x31c4 + (scroll >> 5)`, one tile per 32-pixel column. The byte maps to a
sprite:

- `0` — an empty column.
- `0xFF` — a jump to the 16-bit cs-offset that follows; the stream continues there.
- any other `n` — OUT.BIN catalog cell `n - 1` (catalog record offset `(n-1) * 10`),
  blitted from the catalog segment `0x0F7F`.

Each stream ends in a `0xFF` jump back to its own start, so a layer is a short
repeating pattern looped under the level for its whole length; the level's end comes
from a separate timeline, not the scenery.

### The three layers

| Layer | Tilemap `cs` | Loop | LEVEL_1 catalog cells |
| --- | --- | --- | --- |
| Back  | `0x3137` | 62 columns | sparse in the opening section |
| Mid   | `0x30f2` | 66 columns | 6–14, 71–78 (the lattice) |
| Front | `0x3178` | 49 columns | 1, 2 (the front girders) |

`file = cs + 0x29F0`. The same renderer draws all three from the per-frame compose
path, in order: back, mid, then (after the ship and enemies) front. A layer's depth
is its call position; there is no depth field in the tilemap. Each layer scrolls on
its own accumulator (`cs:0x25f6`, `0x25fa`, `0x25f2`) for the parallax rate.

### Compiled-sprite front pass

LEVEL_1 has a second front pass of compiled sprites, drawn after the ship by routine
`0x8d2b`. It reads a table at `cs:0x43ea` of 18-byte rows `{count, two pieces × four
plane addresses}` and far-calls each piece's compiled blit code in segment `EC00`
(the compiled-sprite format is in `bin.md`). The three tilemap layers cover the
visible lattice; this pass is not yet in the port.

### Port

`scenery.rs` and `assets.rs` decode one loop per tilemap and composite the three
layers over the background, each scrolling at a placeholder rate. The layer rows and
the per-layer scroll rates (`cs:0x25f6/fa/f2`) are placeholders until traced.

## Race levels: the static `0x1690` table

LEVEL_2, 4, 6 are one code build, so a lower-data byte diff between them isolates the
per-level data: a table at file `0x1690`.

- 8-byte records from `0x1698`; slot 0 at `0x1690` is a header of three zero words
  and a per-level word3 (`0xC8` = 200 in LEVEL_2/4, `0x64` = 100 in LEVEL_6; meaning
  unknown).
- Fixed capacity 244 slots (1952 bytes): populated records, then `(0, 0, 0, 20)`
  padding slots (word3 stays 20, the rest zero).
- Populated count scales with level length: LEVEL_2 67, LEVEL_4 82, LEVEL_6 242.
  Every run ends with a shared `(ref, 20, 209, 20)` trailer record; a populated
  record always has a nonzero word0, so the first zero word0 ends the run.
- `word0` is a reference into the level's BIN: in LEVEL_2 the offsets point at real,
  reused sprite data in `RACE1.BIN` (`0x3EB2` appears three times in a row, the same
  sprite placed repeatedly); the bytes there are compiled-sprite code
  (`c6 84` = `mov byte [si+disp16]`) and clip headers. The recurring high word is
  mostly `0x7D00` (with some `0x00FA` / `0x0014`), the same `0x7d00` descriptor marker
  the generated records use.

So the `0x1690` table is the race levels' static object-placement layer (object
class unconfirmed; likely the race obstacle/enemy set, by analogy with the generated
levels). Still open: the meaning of the byte and word after the pointer, and a
render-validation pass.

## Generated levels: the load-time scatter generator

The non-race level layouts are generated, not stored or compressed. On disk the
obstacle buffer holds only a repeating default template `(8, 0x7d00, 0x3308, 10)`;
the real records are written at init by the generator below. Confirmed against the
running game (debugger trace, see Validation).

**Address convention.** Addresses are file-relative: `vaddr = file − 0x200`. At
runtime the WAD's code/data segment sits `0x27F0` higher, so a code operand `cs:[X]`
resolves to **vaddr `X + 0x27F0`** (file `X + 0x29F0`). This matters twice: the
dispatcher's buffer base `bp = 0xcec` is the obstacle buffer at vaddr `0x34dc`, and
the emitters' config reads `cs:[bf6d..bf7b]` hit the depth table at vaddr `0xe75d`.
Everything below is in the file-relative `vaddr` convention.

### The engine PRNG (additive lagged-Fibonacci)

Two functions (addresses are file offsets):

- **Seeder** @ `0x8161`: fills a 58-word table at DGROUP `0x56fb` with an LCG. Per
  entry `x = (x * 0x7ab7 + 0x11) & 0xffff` (multiplier `0x7ab7` = 31415, increment
  `0x11` = 17). Seed is `ax` on entry; the seeder also resets the lag pointers to
  `0x74`/`0x2e`.
- **Generator** @ `0x818c`: additive lagged-Fibonacci over that table. Two lag
  pointers (`si`, `di`, byte offsets) start `0x74` and `0x2e`, step `-2` per call,
  wrap `0 → 0x74`. Output `bx = table[si] + table[di]`, fed back as `table[si] = bx`.
  Modulus comes in `ax` (stored at `[0x56f5]`): nonzero → returns `bx % modulus`,
  zero → raw value (retries if the raw value is 0). Every layout call passes a
  nonzero modulus.

State: DGROUP `0x56f5` (modulus), `0x56f7`/`0x56f9` (saved lag pointers),
`0x56fb`..`0x576f` (the 58-word table). This is the engine's general-purpose RNG
(**46 call sites**); the cluster at vaddr `0xe776`..`0xed8b` is the object scatter.

The wrap target byte `0x74` is word index 58, one past the 58 words the seeder
fills, so the table is effectively 59 words. That 59th slot starts **zero** (it is
read at the first wrap and written by feedback once `si` reaches `0x74`); a non-zero
init breaks the byte-exact match. The static-file value at that address is not what
the runtime sees.

One quirk to replicate: after both lags first reach 0, the generator saves them as
`(0, 0)` again every call, so it reseeds on every subsequent call and the tail of a
very long draw stream goes constant. With LEVEL_1's lag start (period 59/58) the
first both-zero is ~1415 draws in, near the layout's draw count, so a faithful port
must reproduce it.

### Seeding: wall clock, varies per play

The initial seed is **not** fixed. The level-init routine (which calls the layout
dispatcher at file `0xf767`) first calls the time-seeder at file `0xf6f7`:

```asm
mov ah, 0x2c
int 0x21          ; DOS get-time: CH=hour CL=min DH=sec DL=centisec
mov ax, dx        ; ax = (sec << 8 | centisec)
add ax, cx        ; ax += (hour << 8 | min)
cmp ax, 0
jne  seed
mov ax, 0x8ed2    ; fallback seed if the clock sum is 0
seed: call 0x8161
```

So LEVEL_1's scatter is **seeded from the wall clock and varies every play**.
Confirmed by diffing two GET-READY captures taken at different times: the fixed
landmark (record 0) is identical, every rng-driven field differs. The level
*structure* (which sprites, their bands, depths, order) is seed-independent; only the
per-object position scatter changes, which is why the opening looks the same to the
eye. The `0x3039` (12345) reseed value is *not* the initial seed.

The port reproduces the original's seed formula from the local system clock:
`seed = (sec << 8 | centisec) + (hour << 8 | minute)`, with modern centisecond
precision (the original's `DL` stepped in ~5/100s jumps from the 18.2 Hz PIT; the
port does not emulate that quantisation). Same formula, finer input. Not yet wired:
the generator takes a `seed` parameter; the live clock-seeding lands with the level
scene.

The PRNG is bit-for-bit portable: `A=0x7ab7`, `C=0x11`, reseed `12345`, lags
`0x74`/`0x2e`, feedback `X[i] += X[j]`. The GET-READY capture's seed was recovered by
brute force: **`0x3b95`** reproduces all 445 of its records exactly (record 0's x
differs by 1, a 1px scroll). That seed is the regression fixture for the port.

### The dispatcher: a hand-coded layout script

The fill is driven by a **dispatcher** at vaddr `0xed8b`..`0xef6c`. Not a loop over a
data table: a hand-written, straight-line **layout script**, roughly 40 placement
steps, each of the shape

```
mov word cs:[0xbf6b], <screen-x start>   ; where this band begins along the scroll
mov word cs:[0xbf84], <rng base>         ; additive base for the per-object coord
mov word cs:[0xbf82], <rng modulus>      ; spread of the per-object coord
mov ax, <a>  /  mov bx, <b>              ; band object count = rng(a) + b
call <band-emitter>                      ; emit that band into the buffer at bp
```

`bp` is set once to `0xcec` at the top and is the persistent write cursor; every
emitter does `add bp, 8` per record and none resets it, so the bands append into one
growing buffer. `cs:[0xbf6b]` is the running screen-x; an emitter adds it to the first
record's x then zeroes it. `word3` (`y`) is a small per-band vertical jitter,
`rng() % m + base`. `word2` (depth) the emitter reads from `cs:[bf6d..bf7b]` (the
depth table at vaddr `0xe75d`): `100, 160, 500, 600, 200, 200, 1000, 14000, 10000`,
one per sprite type.

### Emitter kinds

The 18 emitters fall into 5 structural kinds (this drives the port's `Emitter` enum):

1. **Single**: `count = rng(a) + b`; per iteration one record, `x = rng(xmod) + xbase
   + xstart`, `y = rng(ym) + yb`. For `0xe776` the x spread comes from the dispatcher
   (`cs:[0xbf82]`/`[0xbf84]`); the others hardcode it.
2. **Row**: like Single but `y` is computed **once per band** (`rng(4) + K` into
   `cs:[0xbf7f]`) and shared. A horizontal row at one height.
3. **Choice**: per iteration roll `rng(5)`; `> 1` (3/5) emits type A, else (2/5) type
   B. One record per iteration; the roll consumes a draw, shifting the stream.
4. **Row + every-Nth**: a Row that also inserts an extra record every 2nd iteration
   (counter in `cs:[0xbf81]`), at `x = 0`, a different sprite type.
5. **Fixed**: no count loop. Emits a hardcoded handful (e.g. `0xeb35` places 2: at
   `x = xstart` and `x = 0x3c`). Landmark pieces; ignores the dispatcher's count.

LEVEL_3 and LEVEL_7 add a **Grid** composition: an inner row wrapped in an outer
count loop that repeats the row N times and resets xstart between rows (see the
LEVEL_3 section for the exact draw order).

**Emitter catalog (LEVEL_1)**: vaddr, kind, sprite ptr (`word1`), depth slot, and the
x / y formulas. `xacc` = `cs:[0xbf6b]` (x-start, added once):

| vaddr | kind | sprite | cfg | x | y |
|-------|------|--------|-----|---|---|
| `0xe776` | Single | `0x3308` | `[bf6d]` | `rng([bf82])+[bf84]+xacc` | `rng(0x12)` |
| `0xe7bb` | Single | `0x38b0` | `[bf6f]` | `rng(0x1e)+0x1e+xacc` | `rng(5)+0x1a` |
| `0xe800` | Single | `0x338e` | `[bf71]` | `rng(0x32)+0x50+xacc` | `rng(5)+0x15` |
| `0xe845` | Single | `0x39a4` | `[bf73]` | `rng(0x32)+0x78+xacc` | `rng(6)+0x36` |
| `0xe88a` | Row | `0x3a92` | `[bf75]` | `0x14+xacc` | `[bf7f]` once |
| `0xe8d5` | Row | `0x33f4` | `[bf77]` | `0x14+xacc` | `[bf7f]` once |
| `0xe920` | Single | `0x3a92` | `[bf75]` | `rng(0x1e)+0x14+xacc` | `rng(6)+0x2c` |
| `0xe965` | Single | `0x33f4` | `[bf77]` | `rng(0x1e)+0x28+xacc` | `rng(6)+0x1f` |
| `0xe9aa` | Choice | `0x338e`/`0x3308` | `[bf71]`/`[bf6d]` | `rng(0xa)+0x1e+xacc` | `rng(5)+0x15` / `rng(0x12)` |
| `0xea2d` | Choice | `0x38b0`/`0x3308` | `[bf6f]`/`[bf6d]` | `rng(0xa)+0x1e+xacc` | `rng(5)+0x1a` / `rng(0x12)` |
| `0xeab0` | Row+everyNth | `0x3a92` (+`0x3308` every 2nd) | `[bf75]` (+`[bf6d]`) | `0x14+xacc` (+`0`) | `rng(4)+0x28` once (+`rng(0x12)`) |
| `0xeb35` | Fixed×2 | `0x392e` | `[bf79]` | `xacc`, then `0x3c` | `0x26`, `0x27` |
| `0xeb72` | Fixed×1 | `0x3f8e` | `[bf7b]` | `xacc` | `0x45` |
| `0xeb92` | Fixed | `0x36ea` | `0xfa` | `xacc` | `0x4b` |
| `0xec39` | Single | `0x3750` | `0xfa` | `xacc` | `rng(3)+0x3c` |
| `0xec65` | Single | `0x37b6` | `0xfa` | `xacc` | `rng(3)+0x3f` |
| `0xec91` | Single | `0x382c` | `0xfa` | `xacc` | `rng(3)+0x42` |
| `0xecbd` | Single | `0x338e` | `[bf71]` | `0x64` | `0x16` |

**LEVEL_1 dispatcher script**: 38 steps, in order. `xstart` carries forward when a
step does not set it. `count = rng(a) + b`:

| # | emitter (vaddr) | xstart | xmod/xbase | count rng(a)+b |
|---|-----------------|--------|------------|----------------|
| 1 | `0xec91` | `0x96` | — | (entry regs) |
| 2 | `0xe776` | `0x96` | `0x1e`/`0x32` | `rng(7)+0x28` |
| 3 | `0xe776` | `0x96` | `0xa`/`0x1e` | `rng(7)+0x8` |
| 4 | `0xea2d` | `0x96` | — | `rng(8)+0x8` |
| 5 | `0xe776` | `0x96` | — | `rng(0xa)+0x8` |
| 6 | `0xe7bb` | `0xc8` | — | `rng(3)+0xc` |
| 7 | `0xea2d` | `0xc8` | — | `rng(5)+0xf` |
| 8 | `0xe800` | `0xc8` | — | `rng(2)+0x6` |
| 9 | `0xe9aa` | `0xc8` | — | `rng(6)+0xa` |
| 10 | `0xec39` | `0x28` | — | `rng(6)+0xa` |
| 11 | `0xe88a` | `0x12c` | — | `rng(5)+0xa` |
| 12 | `0xeb35` | `0x12c` | — | `rng(5)+0xa` (Fixed: count ignored) |
| 13 | `0xe776` | `0x12c` | — | `rng(5)+0x5` |
| 14 | `0xe920` | `0x12c` | — | `rng(5)+0x8` |
| 15 | `0xe7bb` | `0x78` | — | `rng(0xa)+0x14` |
| 16 | `0xec65` | `0x28` | — | `rng(0xa)+0x14` |
| 17 | `0xecbd` | `0x14` | — | `rng(2)+0x2` |
| 18 | `0xe7bb` | `0x14` | — | `rng(0xa)+0x14` |
| 19 | `0xe800` | `0x14` | — | `rng(2)+0x6` |
| 20 | `0xe7bb` | `0x64` | — | `rng(5)+0xa` |
| 21 | `0xe965` | `0x64` | — | `rng(0xa)+0xa` |
| 22 | `0xe776` | `0x64` | — | `rng(5)+0x5` |
| 23 | `0xeab0` | `0x64` | — | `rng(5)+0x5` |
| 24 | `0xe776` | `0x64` | — | `rng(5)+0x5` |
| 25 | `0xeab0` | `0x64` | — | `rng(5)+0x8` |
| 26 | `0xe776` | `0x64` | — | `rng(5)+0xa` |
| 27 | `0xe8d5` | `0x64` | — | `rng(5)+0xa` |
| 28 | `0xe845` | `0xdc` | — | `rng(5)+0xa` |
| 29 | `0xec39` | `0x28` | — | `rng(5)+0xa` |
| 30 | `0xe8d5` | `0xdc` | — | `rng(5)+0xa` |
| 31 | `0xe7bb` | `0xdc` | — | `rng(5)+0xa` |
| 32 | `0xeb72` | `0xfa` | — | `rng(5)+0xa` (Fixed) |
| 33 | `0xe845` | `0xdc` | — | `rng(5)+0xa` |
| 34 | `0xec39` | `0x28` | — | `rng(5)+0xa` |
| 35 | `0xe8d5` | `0xdc` | — | `rng(5)+0xa` |
| 36 | `0xe7bb` | `0xdc` | — | `rng(0x28)+0x14` |
| 37 | `0xe776` | `0xdc` | — | `rng(0xa)+0xa` |
| 38 | `0xeb92` | `0xfa` | — | `rng(0xa)+0xa` (Fixed) |

(`xmod`/`xbase` only matter for `0xe776`, which reads them from `cs:[0xbf82]`/`[0xbf84]`;
later steps leave the last-set values in place. Decode reproducible via
`re/parse_gen.py`.)

### Validation (against the running game)

- **Debugger trace.** A heavy-debug DOSBox-X (`~/dev/dosbox-x`, `build-debug-sdl2`)
  with a `BPM` write-breakpoint on the obstacle buffer stopped on emitter `0xe776`
  building a record: `mov word cs:[bp+2],3308; mov ax,cs:[bf6d] (=0x64); mov
  cs:[bp+4],ax; mov ax,0x12; call <rng>; mov cs:[bp+6],ax; add bp,8`, byte-for-byte
  this catalog, with `bp` walking the buffer and `cs:[bf6d]=0x64` the live depth.
- **GET-READY savestate.** Captured by running `LEVEL_1.WAD` renamed to `.EXE`
  standalone under DOSBox-X (it crashes on the gameplay transition, but GET-READY is
  a stable state with the level data loaded), then saving a state. The `.sav` is a
  zip; its `Memory` member is zlib-compressed RAM. Find the buffer via the C-runtime
  marker `0123456789ABCDEF` (WAD file `0x3694`); the `0x36DA` buffer follows at
  marker + 0x46. The 812 live records match the emitter formulas (the `0x3308` band
  has `x ∈ [0x32,0x50)` = `rng(0x1e)+0x32`, `y ∈ [0,0x12)` = `rng(0x12)`), the
  dispatcher order, and the depths (`0x3308`→100, `0x38b0`→160, `0x338e`→500,
  `0x39a4`→600, `0x33f4`/`0x3a92`→200, `0x392e`→1000, `0x3f8e`→14000). Scripts in
  `re/` (gitignored).

### LEVEL_3: slot-driven emitters and a find-by-position post-pass

LEVEL_3 (WALD) uses the same PRNG and the same x-start-added-once trick, but its
emitters are **generic**: instead of baking a sprite/depth, the dispatcher writes a
set of engine slots before each call and the emitters read them. The slots, at
`cs:[dab7..dac3]`, are x-start (`dab7`), x-step (`dab9`), sprite (`dabb`), depth
(`dabd`), row-reset-x (`dabf`), and row-y offset (`dac3`); some persist across steps
(e.g. the fixed-block emitter reuses the previous step's x-step). The depth table
moves to `cs:[dac5..dad5]`: `[0x82, 0x47e, 0x15e, 0x64, 0x8c, 0x50, 0xbe, 0xc8,
0x3a98]`, one per sprite type.

Emitter kinds (file offsets):

- **Once** (`0x121e7`/`12213`/`1223f`): one record, `x = xstart` (no step), depth
  `0xfa`, `y = rng(3) + base`.
- **Single** (`0x1226b`/`122ab`): `count = rng(a) + b`; per record `x = xstart +
  step` (no x draw), hardcoded sprite + a fixed depth slot, `y = rng(7) + base`.
- **SlotSingle** (`0x123de`/`12434`): like Single but sprite/depth come from the
  slots, and one **dead `rng(0xa)` row-y draw** is burned before the loop (computed
  into `dac1`, unused here, but it advances the stream).
- **Grid** (`0x122eb`/`12367`): `outer = rng(dx) + cx` rows of `inner = rng(ax) + b`
  records (a **zero `ax` modulus skips the inner-count draw**, fixing inner = b). A
  row-y is drawn once per row (`rng(0xa) + 0x25 + dac3` for `122eb`, `rng(0xa) + 0x39`
  for `12367`); after each row xstart resets to `dabf`.
- **FixedRun** (`0x1248a`): no PRNG. A 2-record lead (rec0 `x = xstart + step`, rec1
  `x = 0x32`) then `count` records at `x = 0xf`, all sprite `0x54d6`.

After the 38-step append phase, a **find-by-position post-pass** (`0x12c26` plus a
half-emitter) stamps landmark sprites. Each call walks the built buffer summing
x-steps, finds the record covering a target absolute-x, rewrites that record's x-step,
and overwrites its sprite/depth/y in place. It uses no PRNG. The 28 calls place 6
`0x58b0` near the start (target-x `0x34`..`0x37e`), 21 `0x5ac4` across the mid scroll
(`0x21c0`..`0x39a0`), and one `0x5c20` at the far end (`0x4300`).

Validated the same way as LEVEL_1: the GET-READY buffer is 508 records at the
C-runtime marker + `0x62`, and seed **`0x1a94`** reproduces every one (record 0's x
differs by 1, the 1px scroll). Ported in `crates/game/src/level/slot.rs` (the
slot-model interpreter) and `level_3.rs` (the script, depth table, and post-pass).

### LEVEL_5: shared-row and branch-grid emitters

LEVEL_5 (TECHNO) uses the same slot engine as LEVEL_3, adding three emitter shapes
LEVEL_3 doesn't and no post-pass. Its depths sit in runtime slots `cs:[bd9e..bdae]`,
one per sprite type (`0x3a2c`→`0xfa`/`0x96`, `0x3ac2`→`0x708`, `0x3b70`→`0x10e7`,
`0x3c4e`/`0x3c84`→`0x32`, `0x3cf0`→`0x96`, `0x3d46`→`0x140`/`0x5aa`, `0x426e`→
`0x3a98`); x-start is `cs:[bd16]`, x-step `cs:[bd9c]`, shared row-y `cs:[bdb0]`. The
PRNG is at file `0x97a0`.

New emitter kinds (file offsets):

- **Row** (`0xfd82`/`0xff16`): `count = rng(a) + b` records sharing one y, drawn
  **once** before the loop (vs Single's per-record y). `x = xstart + step`.
- **BranchRows** (`0xffe0`): `rows = rng(a) + b` rows; a single `rng(3)` picks the
  y-pair for every row (result `1` → y `0x47`,`0x48`, else `0x45`,`0x46`). Each row
  emits two `0x3cf0` records, the first `x = xstart + step`, the second `x = 0`.
- **Fixed** (`0x10119`/`0x10146`): no PRNG. A lead record at `x = xstart + step`
  then literal records. Covers the lone `0x3b70` marker and the 6-record `0x426e`
  post-amble (a landmark, then five `0x3764` background rows with stepping y).

Three dispatcher steps wrap their emitter in a `rng(3) + k` repeat loop (the two
grid runs and the `0x3a2c`/`0x3c84` row runs). One r2 caveat: with `-n`, near-call
targets aren't IP-wrapped, so dispatcher calls print `0x10000` high — the real body
offset is `target & 0xffff`.

Validated like the others: the GET-READY buffer is 521 records at the C-runtime
marker + `0x4d`, and seed **`0x2d93`** reproduces every one (record 0's x differs by
1, the 1px scroll). Ported in `slot.rs` (the Row/BranchRows/Fixed variants and the
step-repeat) and `level_5.rs` (the 48-step script and depth constants).

### LEVEL_7: one grid and an inserting post-pass

LEVEL_7 (CITY) has the simplest scatter and the most elaborate post-pass. Its one
PRNG emitter is the L3-shape **Grid** (`0x121bd`): `outer = rng(dx) + cx` rows of
`inner = rng(ax) + bx` records (`ax = 0` skips the inner draw, as in L3 — here
`rng(0)` returns 0 with no state advance), one row-y `rng(0xa) + [cfd9]` per row,
sprite/depth/x-step/row-reset from config slots `cfd1`/`cfd5`/`cfd7`/`cfd3`. The
dispatcher (one straight-line script, file `0x123be`..`0x129b0`) calls it 21 times
between 7 fixed `Once`/landmark emitters (`0x41b5`/`0x421b`/`0x4357` foreground,
`0x4959` blocks).

Two things needed care. First, **x-start (`cs:[0xcf4b]`) is persistent runtime
state**: a Grid leaves it at its row-reset, and a landmark called right after (with
no intervening write) reads that residual, not the dispatcher's last literal.
Second, the post-pass **inserts** rather than overwrites. `0x12381` walks the buffer
summing x-steps, finds the record covering a target absolute-x, and a `rep movsb`
opens a one-record gap there, splitting that record's x-step into
`target - before` / `after - target`. The dispatcher then fills the gap: one op
writes a 5-record `0x7d00` template (overwriting the four shifted records after it),
then eight ops each insert a single `0x4689` landmark (depth `0x140`).

Validated like the others: the GET-READY buffer is 496 records at the C-runtime
marker + `0x5e` (buffer segment offset `0xd02`), and seed **`0x3e94`** reproduces
every one. Ported in `slot.rs` (the `Insert`/`PostOp` post-pass and `Fixed`'s
`lead_step`) and `level_7.rs` (the 28-step script and 9-op insert post-pass).

### Generalized across the generated levels

All four use the identical engine, confirmed by disassembling each one's dispatcher
and emitters. Only data and link addresses differ; the code shape is one engine:

| WAD | PRNG generator | dispatcher config block | distinct emitters |
|-----|----------------|-------------------------|-------------------|
| LEVEL_1 | `0x798c` | `0xbf6b` (x), `0xbf82`/`0xbf84` (cfg) | 18 |
| LEVEL_3 | `0x1bb2a` | `0xdab7` (x), `0xdab9`/`0xdabb` (cfg) | 13 (+ post-pass) |
| LEVEL_5 | `0x97a0` | `0xbd16` (x), `0xbd9c` (cfg) | 16 |
| LEVEL_7 | `0x1bd52` | `0xcf4b` (x), `0xcfd1`..`0xcfd9` (cfg) | 9 (+ insert post-pass) |

Every emitter is the same body: `count = rng() + bx`, then a loop writing
`cs:[bp+0..6] = {x-step, sprite ptr, depth, y}` and `add bp, 8`, with the same
xstart-added-once trick and the same 4-word `0x7d00` descriptor. The dispatcher is the
same straight-line script in all four. The PRNG is the same routine at a relinked
address (the `0x7ab7` multiplier appears exactly once in every WAD). The emitters
range from baked-constant (LEVEL_1 carries its sprite types in the variant) to slot-driven
(LEVEL_3+ read engine slots the dispatcher writes), but it is one interpreter + one
`Emitter` enum + per-level data, no per-level code.

**Ported.** All four validated generators live in `crates/game/src/level/` and share
**one** interpreter: `slot.rs` (the `generate` loop, the `Emitter` enum spanning every
shape from both the baked and slot-driven levels, the overwrite/insert post-pass, and
the step-repeat) plus `prng.rs` (the shared PRNG) and `Record`/`Rand` types. Per-level
data: `level_1.rs` (seed `0x3b95`, `Scatter`/`RowOnce`/`Cells`/`Choice`/`RowEveryNth`),
`level_3.rs` (seed `0x1a94`, 28-call overwrite post-pass), `level_5.rs` (seed `0x2d93`),
`level_7.rs` (seed `0x3e94`, 9-op insert post-pass). Each has a full-buffer golden-hash
test that locks its output byte-for-byte; `re/verify_layouts.py` (with the `dump_layout`
example) re-runs the full record-by-record diff against the captures. The old baked
`generator.rs` is gone: its LEVEL_1-only emitters moved into the slot `Emitter` enum
(renamed off the slot variants they clashed with), so there is no second interpreter.

Both mechanisms hang off the level registry: `LevelData.spawns` is a `SpawnSource`
(`level/spawn.rs`), either the level's script + post-pass fn pointers or the static
table's file offset, and `SpawnSource::records` resolves either into the shared
`Record` buffer (the generated arm takes the PRNG seed, the static arm the WAD
image). The race arm's field mapping is provisional until the render validation.

## Open

- Static landmark records at vaddr `0x5418`: `{0x96,0x3f8e,0x2710,0x45}` and
  `{0xfa,0x3f8e,0x1770,0x46}` in `{x,spr,depth,y}` form, then the default template
  `(8,0x7d00,0x3308,10)`.
- Sprite resolution is solved: `word1` is a cs-offset to an 8-byte descriptor
  `{cell_count, width, height, catalog_index}`; `catalog_index` indexes the OUT.BIN
  catalog `decode_banked` produces, and the sprite spans `cell_count` consecutive
  cells (see `bin.md`). Open: wire resolve-and-blit into the parallax render (place
  at `canyon_x - scroll`, `canyon_y - camera`).
- Race `0x1690`: render-validate (needs `RACE1.BIN`'s catalog offset; the render tool
  hardcodes `OUT_BIN_CATALOG`) and confirm the byte+word fields after the pointer.

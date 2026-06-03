# WAD: level files

Status: container and palette solved (Stage 1). Stage 2: the object-record format
is known (8-byte records, `{x-step, sprite ptr, depth, y}`). Race levels bake the
layout into the EXE (`0x1690` table); the non-race levels generate it at load. The
LEVEL_1 generator (the per-level layout script + emitters + PRNG) is decoded and
validated against the running game; see "The generator" below. Remaining: port it,
and transcribe the other 6 WADs' scripts.

`LEVEL_1.WAD` .. `LEVEL_7.WAD`, one per level. Each is the level's program plus
its embedded palette and data; gameplay assets are loaded from separate disc
files, not packed inside.

## Container: it is a plain DOS MZ EXE

Every WAD is a valid MZ executable, and nothing is appended after the load
image: the header's declared image size equals the file size for all seven, so
there is no overlay or trailing blob. The palette and the level data live
*inside* the loaded image (the program's data, read by its own code), not in a
tacked-on section. The MZ header is 512 bytes (32 paragraphs); code starts at
file offset 512.

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
palette offset. Those are the three RACEB2 race levels, built from one common
code base; the others are each their own build.

## Palette: locate by signature, not by offset

Each WAD embeds the 256-colour palette as a raw 768-byte block of 6-bit VGA
values (the same layout as a `.PAL`). The offset differs per WAD (see the table)
and follows no fixed rule, so it cannot be hardcoded or computed.

It is reliably found by signature instead. The block opens with black then white
then a 30-step descending gray ramp, so the first 32 entries are grayscale. The
locator: scan for `00 00 00 3F 3F 3F` followed by a 768-byte window whose every
byte is `<= 0x3F`. Across all seven WADs this matches in exactly one place each,
so the signature is unambiguous. (It can be tightened further by also requiring
the full 32-entry gray ramp, if a false positive ever turns up.)

## One master palette, two variants

The seven palettes are not seven different ones:

- LEVEL_1, 2, 4, 5, 6 carry a byte-identical **master palette**.
- LEVEL_3 (WALD) is the master palette with only its bottom row changed (foliage
  greens).
- LEVEL_7 (LAVAH) is a more divergent variant (lava oranges and reds).

So the game authored most backgrounds against one shared 256-colour palette and
swapped in tweaked variants only where a level needed colours the master lacked.

Verified by rendering each background with the palette found in its WAD:

- CANYON (master): correct. Tan canyon walls, dark sky, pylons.
- ALIENBG (master): correct. Coherent hazy sky with a bright sun.
- LAVAH (own variant): correct. Glowing orange and red lava.
- WALD (own variant): renders mostly dark with a green-speckled band at the top.
  This is correct: the palette is right (it is the master plus the green bottom
  row, and the master gives the same result), and the dark background is by
  design. The level's vivid colour comes from the foreground trees, which are
  BIN sprites composited on top, not from the background.

## Asset references

The asset filenames the level loads from disc appear as packed, null-terminated
ASCII strings in one region of the WAD (in LEVEL_1 at 0x5137: `out.bin`,
`pturn1.bn1`, `canyon.sp1`..`canyon.sp4`, then the HUD RAWs). They are arguments
the level code passes to DOS file open, not an index into the WAD. The exact
record layout (fixed-width slots vs. plain packed strings) is a Stage 2 detail.

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

Every WAD also references the shared HUD/UI RAW files: `panel.raw`,
`score.raw`, `numbers.raw`, `lights.raw`, `smart.raw`, `extras.raw`,
`balken.raw`, `font.raw`, `0.raw`.

## Stage 2: a per-level data table (consumer still unknown)

LEVEL_2, 4, 6 are one code build (the RACEB2 race levels), so the bytes that
differ between the three are the per-level data. A whole-file byte diff is
muddied in the upper half, where differing data sizes shift the code and its
relocations, but the lower data region diffs cleanly. It isolates a per-level
table at file 0x1690.

What the table looks like in the file (LEVEL_2):

- 8-byte records starting at 0x1698 (slot 0 at 0x1690 is a constant header,
  `00 00 00 00 00 00 C8 00`, the same across the three levels).
- Fixed capacity of 244 slots (1952 bytes): a run of populated records, then
  zero padding.
- The populated count scales with level length: LEVEL_2 ~64, LEVEL_4 ~78,
  LEVEL_6 ~230.
- Each record reads as a far-pointer-shaped value (a 0x3xxx offset and a
  recurring high word, mostly 0x7D00, with some 0x00FA / 0x0014) followed by a
  byte and a word.

This is the structure as it sits in the file. The code that reads it has not
yet been found, so the field meaning above is a data-side reading, not confirmed
from the consumer.

## Stage 2: the Mode X sprite blitter (a separate, traced routine)

A sprite-drawing routine (LEVEL_2 vaddr 0x6744 .. 0x6829, a family of
near-identical loops) is the Mode X blitter. It walks the BIN sprite catalog, not
the `0x1690` table (its loop cursor is screen memory).

What it does:

- The loop's `si` is a VRAM destination cursor: the setup computes
  `si = row * 80 + x` (Mode X is 80 bytes per row), and `si += 8` per step moves
  the screen position 32 pixels right. The `add si, 8` is screen columns, not a
  data-record stride.
- Objects are drawn on a 32-pixel horizontal grid with on-screen culling (x
  between -32 and 288).
- The per-object sprite data is a 10-byte array walked by `bx` (`bx += 0xA`,
  base taken from a control struct via `cs:[di+6]`): a word EMS page id followed
  by four words that are compiled-sprite plane entry points. This 10-byte record
  is exactly the **BIN sprite catalog** already decoded in
  `prototype-formats::bin` (`decode_banked`, e.g. `OUT_BIN_CATALOG`). So this
  loop walks the known sprite catalog, not the `0x1690` table.
- The sprites are compiled code: the blitter patches a `0xCB` (`retf`) into the
  sprite, calls into it, then restores the byte. Four near-identical loop
  variants are selected by a jump table on `x & 3`, the four Mode X pixel phases.
- Sprite pages are banked through EMS: the helper at vaddr 0x56EA maps a page
  with `int 0x67` (`ax = 0x4400`) behind a 4-slot LRU cache (`cs:[0x4F32..]`).

For the Rust port none of the EMS banking or compiled-sprite machinery carries
over (no 640 KB limit); what matters is the data it ultimately places.

## Stage 2: where the scenery layout lives (race levels vs the rest)

Trying to render the `0x1690` records turned up a structural split between the
level builds:

- **The race levels (LEVEL_2, 4, 6) store their scenery layout statically** in
  the `0x1690` table. Its `word0` offsets point at real, reused sprite data in
  `RACE1.BIN` (e.g. `0x3EB2` appears three times in a row: the same scenery
  sprite placed repeatedly). The bytes at those offsets show compiled-sprite code
  (`c6 84` = `mov byte [si+disp16]`) and clip-header tables, so `word0` is a
  reference into the level's BIN. This is consistent with `0x1690` being the
  scenery placement layer, though it is not yet rendered.
- **LEVEL_1 (CANYON) has no such static table.** At the equivalent spot (file
  0x36DA, vaddr `0x34dc`) it holds a buffer filled with a single default record;
  its scenery layout is **generated at load** by the per-level layout script
  decoded below ("The generator").

So the non-race levels generate their layout at load, and the race levels are the
special case that bakes it into a static `0x1690` table.

## Stage 2: LEVEL_1 layout captured at runtime (DOSBox-X savestate)

Static analysis could not find LEVEL_1's layout source (large-model, CS-relative
data, EMS banking, no global `DS`). A runtime snapshot settled the format.

Method: run `LEVEL_1.WAD` renamed to an `.EXE` directly under DOSBox-X (it loads
the level standalone; it crashes on the gameplay transition, but the "GET READY"
screen is a stable state where the level data is already loaded). Save a DOSBox-X
state there. The `.sav` is a zip; its `Memory` member is zlib-compressed RAM.
Decompress, find the runtime location by the C-runtime marker `0123456789ABCDEF`
(at WAD file 0x3694), and the 0x36DA buffer follows at marker + 0x46.

At runtime the buffer is fully populated (the static file holds only the default
template). The records are the `{word0 = x-step, word1 = sprite ptr, word2 = depth,
word3 = y}` format documented in the generator section above (read aligned to the
buffer base at vaddr `0x34dc`). Across LEVEL_1 there are ~11 distinct sprite values
(the canyon scenery set), each placed many times.

`word1` is a **direct OUT.BIN offset** to the scenery sprite's compiled Mode X code
(`c6 84`/`c7 84` plane writes at `0x3308`, `0x33F4`, `0x338E`, ...), not a catalog
index; some sprites sit in a higher EMS page. Plotting the captured records back out
(x = running sum of `word0`, depth from `word2`, y from `word3`) yields a coherent,
themed-by-region canyon (`re/plot_layout.py`).

This 8-byte record shape matches the race levels' `0x1690` table, so both level kinds
share one object-record format: the race levels bake it into the EXE, the others
generate it at load. The runtime bytes do not appear verbatim in any game file, which
first pointed to generation; the generator is now decoded (above). The snapshot and
analysis scripts live in `re/` (gitignored).

## Level architecture (from the original programmer, via the user)

The user, who has the original programmer's notes, described the level model.
Recording it because it reframes several findings:

- `SP1`..`SP4` are the **four Mode X planes of one combined background image**
  (Stage 1 was correct here). That combined image is then **split into several
  horizontal strips**, and each strip scrolls at a **different speed** → the
  parallax depth illusion. One strip is the **foreground**, drawn *in front of*
  the enemies and the player ship.
- The combined image is **taller than the screen**: flying up/down scrolls it
  vertically a little, so the full height is not always visible. The **seams**
  between strips should be visible in the Stage 1 combined render
  (`re/canyon_*`), which is where it gets cut.
- Scenery **objects are attached to the strips** and scroll with them. The
  per-object **depth** (`word2`, fixed per sprite type, from the depth table) is which
  strip / layer the object belongs to, and so its scroll speed. All of one level's
  objects live in the single generated buffer at vaddr `0x34dc`; the depth field
  sorts them onto strips at render time.
- For the non-race levels the object lists are **generated by a scatter algorithm
  at load** (decoded above), not stored. Rationale (the user's): race levels are
  obstacle courses needing hand-picked placement, so they bake an exact table
  (`0x1690`); the other levels' objects are decorative filler a routine populates.

Loader-source decision (in progress): the data-strategy line is **creative
assets** (sprite pixels, audio, palettes, backgrounds: read from the user's disc,
never bundled) vs **the recreated game** (logic plus the design data that drives
it: layout, spawns, timings, attack patterns: the Rust port's own content,
derived by RE, may live in the repo like the rewritten logic). Under this,
baking decoded design data is not the asset-bundling we purged. The non-race levels
are generated, so "loader-source" mostly dissolves: we port the generator and its
small per-layer config (script + depth table); nothing to decompress or extract.

## Stage 2: LEVEL_1's layout is generated at load

The non-race level layouts are procedurally generated, not stored or compressed. On
disk the obstacle buffer holds only a repeating default template
`(8, 0x7d00, 0x3308, 10)`; the real records are written at init by the generator
decoded below. Confirmed against the running game (debugger trace, see Validation).

**Address convention.** Addresses here are file-relative: `vaddr = file − 0x200`. At
runtime the WAD's code/data segment sits `0x27F0` higher than that origin, so a code
operand `cs:[X]` resolves to **vaddr `X + 0x27F0`** (file `X + 0x29F0`). This matters
in two places: the dispatcher's buffer base `bp = 0xcec` is the obstacle buffer at
vaddr `0x34dc`, and the emitters' config reads `cs:[bf6d..bf7b]` hit the depth table
at vaddr `0xe75d`. (The PRNG `call 0x579c` is file `0x818c`.) Everything below is in
the file-relative `vaddr` convention.

### The engine PRNG (additive lagged-Fibonacci)

Found the random generator that drives the scatter. Two functions, addresses are
file offsets (DGROUP vaddr = file − `0x200`):

- **Seeder** @ file `0x8161` (vaddr `0x7f61`): fills a 58-word table at DGROUP
  `0x56fb` with a linear congruential sequence. Per entry:
  `x = (x * 0x7ab7 + 0x11) & 0xffff` (multiplier `0x7ab7` = 31415, increment
  `0x11` = 17). The seed is `ax` on entry.
- **Generator** @ file `0x818c` (vaddr `0x798c`): additive lagged-Fibonacci over
  that table. Two lag pointers (`si`, `di`, byte offsets) start `0x74` and `0x2e`
  apart, step `-2` each call, wrap `0 → 0x74`. Output `bx = table[si] + table[di]`,
  fed back as `table[si] = bx`. When **both** pointers wrap to 0 it reseeds the
  table via the seeder with seed `0x3039` = **12345**. The call takes a modulus in
  the `[0x56f5]` slot: nonzero → returns `bx % modulus` (bounded), zero → raw
  value (and retries if the raw value is 0, so raw never returns 0).

State lives in DGROUP `0x56f5` (modulus), `0x56f7`/`0x56f9` (saved lag pointers),
`0x56fb`..`0x576f` (the 58-word table). The generator is the engine's
general-purpose RNG: **46 call sites**. The dense cluster at vaddr
`0xe776`..`0xed8b` is the object-scatter code (see next section).

This is portable bit-for-bit: constants `A=0x7ab7`, `C=0x11`, seed `12345`, lags
at `0x74`/`0x2e` over a 58-word ring, feedback `X[i] += X[j]`. Replicating it
reproduces the original's exact random stream and therefore the exact layout.

### The generator: a hand-coded per-level layout script

The obstacle buffer is at vaddr `0x34dc` (file `0x36dc`); the generator's
`mov bp, 0xcec` targets it (runtime offset `0xcec` = vaddr `0x34dc`, per the address
convention above).

The fill is driven by a **dispatcher** at vaddr `0xed8b`..`0xef6c` (file
`0xef8b`..`0xf16c`). It is not a loop over a data table. It is a hand-written,
straight-line **layout script** for LEVEL_1: roughly 40 placement steps, each of
the shape

```
mov word cs:[0xbf6b], <screen-x start>   ; where this band begins along the scroll
mov word cs:[0xbf84], <rng base>         ; additive base for the per-object coord
mov word cs:[0xbf82], <rng modulus>      ; spread of the per-object coord
mov ax, <a>  /  mov bx, <b>              ; band object count = rng() + b (a is a 2nd modulus)
call <band-emitter>                      ; emit that band into the buffer at bp
```

`bp` is set once to `0xcec` at the top and is the persistent write cursor; every
emitter does `add bp, 8` per record and none resets it, so the bands append into
one growing buffer. `cs:[0xbf6b]` is the running screen-x; an emitter adds it to
the first record's x then zeroes it.

The record is 8 bytes / 4 words: `{word0 = x-step, word1 = sprite-descriptor pointer,
word2 = depth, word3 = y}`. `word0` is a horizontal **step**: the emitter computes
`rng(xmod) + xbase` (plus the x-start `cs:[0xbf6b]` on a band's first record, then
zeroes it), and the consumer running-sums these to the absolute x along the scroll
(CANYON totals ~11,800 px, ~37 screens). `word3` (`y`) is a small per-band vertical
jitter, `rng() % m + base`. `word2` is a per-sprite-type **depth / draw-layer** the
emitter reads from `cs:[bf6d..bf7b]` (the depth table at vaddr `0xe75d`):
`100, 160, 500, 600, 200, 200, 1000, 14000, 10000`, one per type.

**Emitter kinds.** The 18 emitters are not one shape with different constants.
They fall into 5 structural kinds (this is the fact that drives the port's
`Emitter` enum):

1. **Single**: `count = rng(a) + b` (a, b from the dispatcher's `ax`/`bx`); per
   iteration emit one record, `x = rng(xmod) + xbase + xstart`, `y = rng(ym) + yb`.
   For emitter `0xe776` the x spread `xmod`/`xbase` come from `cs:[0xbf82]`/`[0xbf84]`
   (set by the dispatcher); the others hardcode them.
2. **Row**: like Single but `y` is computed **once per band** (`rng(4) + K` into
   `cs:[0xbf7f]`) and shared by every record. A horizontal row at one height.
3. **Choice**: per iteration roll `rng(5)`; `> 1` (3/5) emits object type A, else
   (2/5) type B. Each type has its own sprite ptr / config / x / y. One record per
   iteration, but the roll consumes a PRNG draw, so it shifts the stream.
4. **Row + every-Nth**: a Row that also inserts an extra record every 2nd
   iteration (counter in `cs:[0xbf81]`), at `x = 0`, a different sprite type.
5. **Fixed**: no count loop. Emits a hardcoded handful of records (e.g. `0xeb35`
   places exactly 2: at `x = xstart` and `x = 0x3c`). Landmark/structure pieces;
   ignores the dispatcher's count params.

**Emitter catalog (LEVEL_1)**: vaddr, kind, sprite ptr (`word1`), depth slot
(`word2`, read from `cs:[bf6d..bf7b]`), and the x / y formulas. `xacc` =
`cs:[0xbf6b]` (x-start, added once):

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

The sprite-ptr constants (`0x3308`, `0x38b0`, ...) are pointers into a descriptor
region; `0x392e`/`0x3f8e` themselves point at `0x3308`-style values (nested), so
`word1` may be a pointer to a sprite-record rather than the sprite itself. Confirm
when wiring the consumer.

**LEVEL_1 dispatcher script**: 38 steps, in order. `xstart` carries forward when
a step does not set it. `count = rng(a) + b`:

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

(`xmod`/`xbase` only matter for emitter `0xe776`, which reads them from
`cs:[0xbf82]`/`[0xbf84]`; later steps leave the last-set values in place. The
dispatcher decode is reproducible via `re/parse_gen.py`.)

**Validation (against the running game).** Two independent checks confirm this decode:

- **Debugger trace.** A heavy-debug DOSBox-X (`~/dev/dosbox-x`, `build-debug-sdl2`)
  with a `BPM` write-breakpoint on the obstacle buffer stopped on emitter `0xe776`
  building a record: `mov word cs:[bp+2],3308; mov ax,cs:[bf6d] (=0x64); mov
  cs:[bp+4],ax; mov ax,0x12; call <rng>; mov cs:[bp+6],ax; add bp,8`, byte-for-byte
  this catalog, with `bp` walking the buffer and `cs:[bf6d]=0x64` the live depth.
- **GET-READY savestate** (`re/save`, tools `re/live.py`/`re/verify.py`). The 812
  live records match the emitter formulas (the `0x3308` band has `x ∈ [0x32,0x50)` =
  `rng(0x1e)+0x32`, `y ∈ [0,0x12)` = `rng(0x12)`), the dispatcher order (record 0 is
  `spr 0x382c` from step 1; the Choice emitter shows as interleaved `0x38b0`/`0x3308`
  rolls), and the depths (`0x3308`→100, `0x38b0`→160, `0x338e`→500, `0x39a4`→600,
  `0x33f4`/`0x3a92`→200, `0x392e`→1000, `0x3f8e`→14000).

**Porting.** Faithful reconstruction needs: (1) the PRNG, (2) this dispatcher script
per WAD, (3) the 5-kind emitter library with each emitter's constants, (4) the depth
table. Run them against the seeded PRNG and the byte-exact original layout falls out.

## Open (Stage 2 continued)

- LEVEL_1 generator is decoded and validated. Remaining: transcribe the other 6
  WADs' dispatcher scripts the same way (`re/parse_gen.py`, given each WAD's
  dispatcher/emitter offsets), then build the Rust 5-kind emitter library +
  interpreter (faithful-by-algorithm: PRNG + script + depth table → byte-exact layout).
- `word1` (sprite-descriptor pointer): its target is a 4-word descriptor
  `(0x0008, 0x7d00, 0x3308, 0x000a)` where `0x7d00` is a sprite-data / OUT.BIN offset.
  The pointer-vs-index resolution is a render-side detail to pin when wiring sprites.
- x-start detail: confirm exactly when `cs:[0xbf6b]` adds to record `x`. The code adds
  it then zeroes it, so it applies to the first record of a band only; the live
  `0x3308` band (`x ∈ [0x32,0x50)` = `rng(0x1e)+0x32`) is consistent with that. Verify
  against the emitter when porting so the x stream matches bit-for-bit.
- Static landmark records exist at vaddr `0x5418`: `{0x96,0x3f8e,0x2710,0x45}` and
  `{0xfa,0x3f8e,0x1770,0x46}` in `{x,spr,depth,y}` form (the Fixed `0x3f8e` object),
  then the repeating default template `(8,0x7d00,0x3308,10)`.
- Pixel render: decode the Mode X compiled sprites at each `word2` OUT.BIN offset
  (4 planes) and draw them over the CANYON background, replacing the colour-bar
  placeholders. Format is validated; this is the remaining visual polish.
- The EMS page for `word2` values that fall in higher pages (some hit zero at the
  flat OUT.BIN offset).

- Render-validate the race `0x1690` table: needs `RACE1.BIN`'s catalog offset
  (the render tool currently hardcodes `OUT_BIN_CATALOG`), then place its records
  over `RACEB2` to confirm the scenery interpretation and the byte/word fields.
- The `0x1690` record field meaning (the byte and word after the pointer),
  confirmed from a render or the consumer.
- The exact filename-record layout in the asset-reference region.

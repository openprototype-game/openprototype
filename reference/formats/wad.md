# WAD: level files

Status: container and palette solved (Stage 1). Stage 2: the object-record format
is known (8-byte records, two position fields + sprite + a per-sprite field).
Race levels bake the table into the EXE (0x1690); the others fill it at load.
LEVEL_1's 446-record buffer was captured at runtime via a DOSBox-X savestate.
Current task: find the filler code that populates that buffer, to settle whether
the non-race levels are generated (likely) or decompressed. See "Level
architecture" below for the parallax model and the pending retcons.

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
from the consumer. (An earlier version of this note claimed a specific blit loop
walked this table; that was wrong, see below.)

## Stage 2: the Mode X sprite blitter (a separate, traced routine)

While chasing the table I traced a sprite-drawing routine (LEVEL_2 vaddr
0x6744 .. 0x6829, a family of near-identical loops). It is worth recording, but
it is **not** confirmed to consume the 0x1690 table: I reached it by a byte
coincidence, and its loop cursor turned out to be screen memory, not the table.

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
- **LEVEL_1 (CANYON) has no such static table.** At the equivalent spot it holds
  an 800-slot buffer (LEVEL_1 file 0x36DA) filled with a single default record,
  so its scenery layout is populated at runtime from another source. That source
  is not a `0x7D00` placement table inside `OUT.BIN` either (OUT.BIN is ~2 MB of
  mostly compiled-sprite code). Finding it needs dynamic tracing (watch what
  fills the 0x36DA buffer at level load), not more static scanning.

So the non-race levels likely share the runtime-loaded scheme, and the race
levels are the special case that bakes the layout in. The race `0x1690` table is
the one concrete, static level-data format found so far.

## Stage 2: LEVEL_1 layout captured at runtime (DOSBox-X savestate)

Static analysis could not find LEVEL_1's layout source (large-model, CS-relative
data, EMS banking, no global `DS`). A runtime snapshot settled the format.

Method: run `LEVEL_1.WAD` renamed to an `.EXE` directly under DOSBox-X (it loads
the level standalone; it crashes on the gameplay transition, but the "GET READY"
screen is a stable state where the level data is already loaded). Save a DOSBox-X
state there. The `.sav` is a zip; its `Memory` member is zlib-compressed RAM.
Decompress, find the runtime location by the C-runtime marker `0123456789ABCDEF`
(at WAD file 0x3694), and the 0x36DA buffer follows at marker + 0x46.

At runtime the 800-slot buffer is fully populated (the static file holds only a
default record). LEVEL_1 holds a header record then **446 object records**, then
default-record fill to slot 800. Each record is 8 bytes, four little-endian
words:

- `word2`: the sprite. Only 11 distinct values across the level (the canyon
  scenery set), each placed many times.
- `word3`: a per-sprite property, fixed per `word2` (0x3308 -> 100, 0x38B0 ->
  160, ...). First read as width, but given the parallax model below it is more
  likely the **depth / scroll-speed / layer** the sprite belongs to. Retcon
  candidate; not yet confirmed.
- `word0`: the **horizontal step** to this object. Its running sum is the x
  position along the scrolling level (CANYON totals ~11,759 px, ~37 screens).
- `word1`: the **vertical position** (0..163).
- header (record 0) = `(0, 149, 0x382C, 250)`.

`word2` is a **direct OUT.BIN offset** to the scenery sprite's compiled Mode X
code (`c6 84`/`c7 84` plane writes at 0x3308, 0x33F4, 0x338E, ...), not a catalog
index; some sprites sit in a higher EMS page. Field meanings were confirmed by
plotting the 434 captured records back out (x = running sum of `word0`, y =
`word1`, colour = `word2`, width = `word3`): the result is a coherent,
themed-by-region canyon layout (`re/plot_layout.py`, `re/canyon_layout.png`).

This 8-byte record shape matches the race levels' `0x1690` table, so both level
kinds share one object-record format; the race levels bake it into the EXE and
the others decompress it in at load.

The exact runtime bytes do **not** appear verbatim in any game file (LEVEL_1.WAD,
OUT.BIN, the other BINs), so the level is built or decompressed at load. The
snapshot lives in `re/` (gitignored); the analysis scripts are `re/*.py`.

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
- Scenery **objects are attached to the strips** and scroll with them. This
  explains the multiple object tables the directory at `0x56e6` points at
  (`0x34da`, `0x3442`, `0x3492`, ...): they are the per-strip object lists. So
  the `0x34da` 446-record buffer is **one strip's objects, not the whole level**.
- For the non-race levels the object lists are almost certainly **generated by a
  scatter algorithm at load**, not stored. Rationale (the user's): race levels
  are obstacle courses needing hand-picked placement, so they bake an exact table
  (`0x1690`); the other levels' objects are decorative filler with no reason to
  author or store, so a routine populates them. This is the leading hypothesis
  for what the filler does.

Pending retcon this implies (the user has OK'd retconning earlier commits):
- `word3` may be the **strip / depth / scroll-speed** the object belongs to
  rather than width (it is fixed per sprite type, which fits either). Not yet
  confirmed. (Stage 1's four-planes reading is correct, no retcon there.)

Loader-source decision (in progress): the data-strategy line is **creative
assets** (sprite pixels, audio, palettes, backgrounds: read from the user's disc,
never bundled) vs **the recreated game** (logic plus the design data that drives
it: layout, spawns, timings, attack patterns: the Rust port's own content,
derived by RE, may live in the repo like the rewritten logic). Under this,
baking decoded design data is not the asset-bundling we purged. If the levels are
generated (likely), "loader-source" mostly dissolves: we port the generator and
its small per-layer config; nothing to decompress or extract.

## Stage 2 resolved: the layout is generated, not stored

Settled the generated-vs-compressed question by comparing the WAD file bytes at
the buffer's load address against the runtime RAM:

- The buffer lives at DGROUP offset `0x34DA`. The MZ header is `0x200` bytes, so
  that maps to file offset `0x36DA`. The file there is **not** the records: it is
  the default sentinel tuple `(8, 0x7d00, 0x3308, 10)` repeated, with `(0, ...)`
  in record 0. The file ships the buffer as a **template** (BSS-style initialized
  data); the runtime holds the 446 varied records. So the loader **writes** the
  buffer at init: generated, not loaded, not decompressed.

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
general-purpose RNG: **46 call sites**. A dense cluster at vaddr `0xe777`..`0xeab5`
(many RNG calls packed together) is the likely object-scatter routine / buffer
filler; not yet traced end-to-end to the `0x34da` write.

This is portable bit-for-bit: constants `A=0x7ab7`, `C=0x11`, seed `12345`, lags
at `0x74`/`0x2e` over a 58-word ring, feedback `X[i] += X[j]`. Replicating it
reproduces the original's exact random stream and therefore the exact layout.

### Caveat on the earlier "directory at 0x56e6"

The `0x56fb`..`0x576f` region I'd read as a directory of per-strip pointers is in
fact this PRNG **state table**, overwritten at load by the seeder. The `da 34`
(= `0x34da`) bytes seen there in the *file* are the static initial image of that
region, not a live pointer table. There are still `0x34da` references in the file
**beyond** the table (≥ `0x577f`) that may be a real per-strip pointer list; that
needs re-examination separately. The three strip buffers `0x34da`/`0x3442`/`0x3492`
and the consumer pointer-setup (`mov word [di], 0x34da` at vaddr `0xc28c`/`0xd2ad`)
still stand.

## Open (Stage 2 continued)

- Trace the filler: confirm the `0xe7xx` RNG cluster writes the `0x34da` buffer,
  and read the per-strip config it consumes (sprite id, count, y-band, x-spacing)
  so the generator can be ported with its inputs.
- Re-examine the `≥0x577f` `0x34da` references now that `0x56fb`+ is known to be
  PRNG state, to find the real per-strip pointer/config list (if any).
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

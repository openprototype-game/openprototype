# Level object layout

How each level decides where its scenery and obstacle objects sit. Two
mechanisms, one record format:

- **Race levels (2, 4, 6)** bake a static placement table into the EXE (`0x1690`).
- **Generated levels (1, 3, 5, 7)** build the layout at load from a PRNG-driven
  layout script. The LEVEL_1 generator is fully decoded and validated against the
  running game; the same engine is confirmed in 3, 5, 7 by disassembly.

Both kinds emit the same 8-byte object record. The WAD container itself (MZ image,
palette, asset strings) is in `wad.md`; this doc is only the layout system.

Remaining: port the generator and transcribe the per-level data (script + emitter
table + depth table) for 3/5/7.

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
- **word2 (depth)** is a per-sprite-type draw layer: which parallax strip the object
  rides, and so its scroll speed (see Level architecture). Fixed per sprite type.
- **word3 (y)** is the vertical position.

The same 8-byte shape appears in the race levels' static `0x1690` table, so both
level kinds share one object-record format.

Runtime buffer base: the records start at the C-runtime marker + `0x48` (the earlier
+`0x46` was one word low, which is what made the fields look reordered). Read there,
the runtime records are exactly `{x, sprite, depth, y}`, matching the write trace.

## Level architecture (parallax strips)

From the original programmer's notes (via the user). It reframes several findings:

- `SP1`..`SP4` are the **four Mode X planes of one combined background image**. That
  image is **split into horizontal strips**, each scrolling at a **different speed**
  for the parallax depth illusion. One strip is the **foreground**, drawn *in front
  of* the enemies and the player ship.
- The combined image is **taller than the screen**: flying up/down scrolls it
  vertically, so the full height is not always visible. The strip **seams** show in
  the Stage 1 combined render (`re/canyon_*`), where it gets cut.
- Scenery **objects are attached to the strips** and scroll with them. The per-object
  **depth** (`word2`) is which strip/layer the object belongs to. All of one level's
  objects live in the single buffer at vaddr `0x34dc`; depth sorts them onto strips
  at render time.
- The non-race levels **generate** their object lists at load; the race levels bake an
  exact table. Rationale (the user's): race levels are obstacle courses needing
  hand-picked placement; the others' objects are decorative filler a routine scatters.

Data-strategy note: decoded design data (layout scripts, depth tables) is the
recreated game's own content, not the creative assets (sprite pixels, audio,
palettes) we read from the user's disc. For the generated levels there is nothing to
extract anyway: we port the generator and its small per-layer config.

## Race levels: the static `0x1690` table

LEVEL_2, 4, 6 are one code build, so a lower-data byte diff between them isolates the
per-level data: a table at file `0x1690`.

- 8-byte records from `0x1698`; slot 0 at `0x1690` is a constant header
  `00 00 00 00 00 00 C8 00`, identical across the three.
- Fixed capacity 244 slots (1952 bytes): populated records, then zero padding.
- Populated count scales with level length: LEVEL_2 ~64, LEVEL_4 ~78, LEVEL_6 ~230.
- `word0` is a reference into the level's BIN: in LEVEL_2 the offsets point at real,
  reused sprite data in `RACE1.BIN` (`0x3EB2` appears three times in a row, the same
  scenery sprite placed repeatedly); the bytes there are compiled-sprite code
  (`c6 84` = `mov byte [si+disp16]`) and clip headers. The recurring high word is
  mostly `0x7D00` (with some `0x00FA` / `0x0014`), the same `0x7d00` descriptor marker
  the generated records use.

So the `0x1690` table is the race levels' static scenery-placement layer. Still open:
the meaning of the byte and word after the pointer, and a render-validation pass.

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
init breaks the byte-exact match. The earlier static-file value at that address is
not what the runtime sees.

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

The port seeds the generator from the clock at level start, faithfully. Bit-for-bit
portable: `A=0x7ab7`, `C=0x11`, reseed `12345`, lags `0x74`/`0x2e`, feedback
`X[i] += X[j]`, init seed from the clock. The GET-READY capture's seed was recovered
by brute force: **`0x3b95`** reproduces all 445 of its records exactly (record 0's x
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

LEVEL_7 adds a **Grid** composition: a Row wrapped in an outer count loop that
repeats the row N times and resets xstart between rows. The model needs a repeat-row
kind (a sixth `Grid`, or a `rows` count on `Row`).

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

### Generalized across the generated levels

LEVEL_3, 5, 7 use the identical engine, confirmed by disassembling each one's
dispatcher and emitters. Only data and link addresses differ; the code shape is one
engine:

| WAD | PRNG generator | dispatcher config block | distinct emitters |
|-----|----------------|-------------------------|-------------------|
| LEVEL_1 | `0x798c` | `0xbf6b` (x), `0xbf82`/`0xbf84` (cfg) | 18 |
| LEVEL_3 | `0x1bb2a` | `0xdab7` (x), `0xdab9`/`0xdabb` (cfg) | ~7 |
| LEVEL_5 | `0x97a0` | `0xbd16` (x), `0xbd9c` (cfg) | ~13 |
| LEVEL_7 | `0x1bd52` | `0xcf4b` (x), `0xcfd1`..`0xcfd9` (cfg) | ~8 |

Every emitter is the same body: `count = rng() + bx`, then a loop writing
`cs:[bp+0..6] = {x-step, sprite ptr, depth, y}` and `add bp, 8`, with the same
xstart-added-once trick and the same 4-word `0x7d00` descriptor. The dispatcher is the
same straight-line script in all four. The PRNG is the same routine at a relinked
address (the `0x7ab7` multiplier appears exactly once in every WAD). The emitters are
per-level *code* with their scenery constants compiled in, but every one is an
instance of the shared kinds, so the port is one interpreter + the `Emitter` enum +
per-level data, no per-level code.

**Ported.** LEVEL_1's generator is implemented in `crates/game/src/level/`
(`prng.rs`, `generator.rs` with the `Emitter` enum + interpreter, `level_1.rs` with
the 38-step script and depth table). A test asserts seed `0x3b95` reproduces the
validated record set.

## Open

- Transcribe the per-level data for 3/5/7 (dispatcher script + emitter table + depth
  table) via `re/parse_gen.py` given each WAD's offsets (table above) and feed it to
  the shared interpreter. Recover each one's capture seed the same way to validate.
- Static landmark records at vaddr `0x5418`: `{0x96,0x3f8e,0x2710,0x45}` and
  `{0xfa,0x3f8e,0x1770,0x46}` in `{x,spr,depth,y}` form, then the default template
  `(8,0x7d00,0x3308,10)`.
- Pixel render: decode the Mode X compiled sprites at each `word2` OUT.BIN offset
  (see `bin.md`) and draw them over the background, replacing the colour-bar
  placeholders. The EMS page for `word2` values in higher pages (some hit zero at the
  flat OUT.BIN offset).
- Race `0x1690`: render-validate (needs `RACE1.BIN`'s catalog offset; the render tool
  hardcodes `OUT_BIN_CATALOG`) and confirm the byte+word fields after the pointer.

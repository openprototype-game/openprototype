# START.EXE: front-end / launcher map

Status: first pass. Skeleton confirmed by disassembly (r2); inner routines
(menu draw, input loop, asset loader) not yet traced.

START.EXE is the menu front-end. It sets up video, shows the menu/intro, and
launches a level by EXEC-ing its `level_N.wad` directly. Each `level_N.wad` is
a standalone DOS executable (the game engine plus that level's data), so there
is no separate engine binary. The `proto13.exe` string in the binary is a dead
dev leftover: it has zero code references (confirmed by reference search),
which is why it is not on the original ISO.

## Format

- DOS MZ, 16-bit real mode, x86. 22064 bytes, header 512 bytes (0x200).
- Entry CS:IP = 0000:4705 (r2 `entry0`). The old note's "cs offset 0x200" is
  just the MZ header size; it applies to every EXE here.
- No mouse: no `int 0x33` anywhere. Keyboard-driven.

## Confirmed landmarks

| What | Where | Evidence |
|------|-------|----------|
| Entry | `entry0` @ 0x4705 | MZ header |
| Drive-letter fixup | 0x470b: `int 21 AH=19` (get drive), `al+0x61`, patches the `x:` byte in path strings | disasm |
| VGA mode 13h set | 0x4788: `AX=0013; int 0x10` | disasm |
| VBlank wait | just before, polls port 0x3DA bit 3 | disasm |
| Program launcher | `fcn.00002aff`: copies command tail, `AX=4B00; int 0x21` @ 0x2b31, then `AH=4D` exit code | disasm |
| Drive-switch wrapper | `fcn.00002adc`: `int 21 AH=0E` to game drive, launch, switch back | disasm |
| Level launch | `fcn.00003abe(level)`: index 16-byte table at paddr 0x3684 -> `level_N.wad`, drive letter as command tail | disasm |
| Menu string table | 0x050f, `$`-delimited (NEW GAME, LOAD GAME, ... HIGHSCORES, credits) | strings |
| Asset filename table | 0x3884+, fixed-width `x:`-prefixed entries | strings layout |

## The `x:` path convention

Path strings are stored with a literal `x:` prefix (`x:cover3.bdy`,
`x:fli\canyon.fli`). At startup the code reads the current DOS drive
(`int 21 AH=19`), turns it into a letter (`+ 0x61`), and overwrites the `x`
byte. So `x:` means "the drive the game runs from".

## Asset filename table (around 0x3884)

Consecutive entries, used by the launcher and loader:

- `level_1.wad` .. `level_7.wad` (0x3884+): the level engines launched via
  `fcn.00002aff`.
- `x:fli\canyon.fli`, `space1`, `waldende`, `space2`, `tend`, `space3`,
  `lava`: intro/inter-level cutscenes.
- `x:cover3.bdy` / `.pal`, `x:neo.bdy`, `x:surplogo.bdy`: the title and logo
  images (already decodable; COVER3 is the cover art we rendered).
- `x:font.raw`, `x:font2.raw`, `x:back3.raw`: menu font and background.

## Subsystems (inventory)

By interrupt usage and call graph (93 functions total):

- **Startup**: `fcn.00000010` shrinks the memory block (`AH=4A`); drive-letter
  fixup; loads `proto.cfg` (the "not found" string). entry0 is the driver.
- **EMS memory**: `int 0x67`, allocates the ~2.5 MB "Grafik Data" buffer
  (matches the EMS error strings).
- **Interrupt handlers**: installs custom ISRs via `AH=25` at 0x1024, 0x103c,
  0x108d, 0x109a, 0x2e9b (timer/keyboard); restored on exit (`AH=35` get,
  `AH=25` set).
- **Video**: mode 13h via `int 0x10 AX=0013` (re-set after each full-screen
  image/FLI); palette uploaded through the DAC port 0x3C8/0x3C9 by the routine
  at 0x0208-0x0240 (`fcn.00000230`); pixel output written to segment 0xA000.
- **File loading**: `int 0x21` AH=3D/3F/3E/42 (open/read/close/seek). Loads
  the BDY/PAL/RAW assets from the table at 0x3684.
- **Sound**: SoundBlaster DSP init (the "DMA-Buffer"/"DSP-Version" strings);
  not yet located precisely.
- **FLI playback**: the intro/cutscene player; not yet located precisely.
- **Menu + submenus**: render (font.raw + back3.raw), input loop, and the
  graphics/joystick/volume/music/load/save/highscores/credits screens.
- **Level launch**: see below (resolved).

Decompiling all 93 functions to port-ready detail is an iterative job;
this inventory is the structural pass. Take one subsystem at a time.

## Menu render + input (resolved)

The main menu lives in `fcn.00003e41` (called from `entry0` @ 0x4b02). It draws
into the mode-13h framebuffer at segment 0xA000.

### Text / glyph drawing: `fcn.00003d03(al = ASCII char)`

`di` = destination screen offset, `es` = 0xA000. Steps:

- glyph index = `char - 0x20` (space is the first glyph).
- The font is a sheet (`font.raw`, 320 px wide): **20 glyphs per row, each
  16 px wide and 15 px tall, glyph area starts at y = 16** (source base
  `si = 0x1400`). For index `n`: advance `0x12c0` (= 320*15) per group of 20,
  then `+ n*16` within the row.
- Copies 15 rows of 16 px, source/dest stride 320. **Pixel value 0 is
  transparent** (left as background). Then `di += 16` to the next cell.

So a level WAD's HUD font and this menu font share the same layout idea; worth
a `font` decoder in `crates/formats` later.

### Key input: `fcn.00002e5c()` (blocking)

Spins until `cs:[0x2dec] != 0`, returns the scancode from `cs:[0x2ded]`, clears
the flag. The flag+scancode are written by the custom `int 9` keyboard ISR
(installed via the `AH=25` sites). Returns a raw scancode, not ASCII.

### Menu loop: `fcn.00003e41`

- 5 items at framebuffer offsets `0x4b46, 0x5f46, 0x7346, 0x8746, 0x9b46`,
  spaced `0x1400` (= 16 scanlines). At 320 wide that is x≈70, y = 60/76/92/108/124.
- Each iteration redraws the `'>'` cursor (char 0x3e) at the current item via
  `fcn.00003d03`, then blocks on `fcn.00002e5c`.
- Scancodes: **0x48 = Up, 0x50 = Down** (move cursor by `0x1400`, wrapping at
  the first/last item), **0x1C = Enter** (dispatch).
- Enter dispatches by current offset: 0x4b46 → NEW GAME (0x4b05), 0x5f46 →
  0x4258, 0x7346 → options `fcn.00003f0c`, 0x8746 → 0x439a, 0x9b46 → QUIT
  (0x4d90, the `AH=4C` exit).

The item labels themselves (from the 0x030f `$`-table) are drawn once before
the loop, with the same `fcn.00003d03`.

## Level launch contract (resolved)

`fcn.00003abe(bx = level number)`:

```
dec bx; shl bx, 4; add bx, 0x3684   ; table[level-1], 16 bytes/entry
mov dx, bx                          ; DX = "level_N.wad" path
mov si, 0x2acd                      ; SI = command tail = drive letter
call fcn.00002adc                   ; switch to game drive, EXEC, switch back
```

Called from `entry0` @ 0x4b48 and 0x4b6f (the menu's level-select paths). The
launched WAD receives the drive letter as its command tail.

## Open threads (next digs)

- Which routine loads `cover3.bdy` / `back3.raw` / `font.raw` and blits them.
- The menu state machine: how items in the 0x050f table map to actions, and
  the keyboard input loop.
- The other `int 0x10` sites (0x47c4, 0x4902, 0x4a6f, ...): palette/page
  setup vs. additional mode changes.

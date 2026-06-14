# START.EXE: front-end / launcher

The menu front-end and level launcher: it plays the intro, runs the menu, and
`EXEC`s `level_N.wad` to play a level. Each `level_N.wad` is a standalone DOS
program (the engine plus that level's data), so there is no separate engine
binary. The `proto13.exe` string is a dead dev leftover (zero code references).

This file is both the structural reference and the runtime flow. The port's
front-end implements all of it; the mapping to port modules is at the end.

Addresses are r2 flat vaddr (`file = vaddr + 0x200`, the MZ header). Traced in
r2 6.1.4.

## Format

- DOS MZ, 16-bit real mode, x86. 22064 bytes, header 512 bytes (`0x200`).
- Entry `CS:IP = 0000:4705` (r2 `entry0`).
- No mouse (no `int 0x33`): keyboard-driven, through a custom `int 9` ISR.
- The CD-audio music driver is a separate code segment (`0x4dc`), reached
  through thunks at `0x265`..`0x308`.

## Process model

Front-end and level runtimes are separate programs. START.EXE starts the title
music, plays the intro, runs the menu, and on NEW GAME drives a level-by-level
state machine that `EXEC`s each WAD and reads its outcome back from a shared
`message` file. The port mirrors this as a front-end state machine plus a
level runtime in one process.

## Top-level flow

```
entry0 (0x4705)
  │
  ├─ BOOT
  │    mem-shrink · load high.txt · drive-letter fixup · read drive.nfo and
  │    fli.nfo (exit if missing) · load proto.cfg · joystick detect
  │    EMS check + alloc the ~2.5 MB buffer · build palette · delete a stale
  │    message file · install ISRs · set VGA mode 13h · load title assets
  │
  ├─ TITLE / INTRO  (0x47db .. 0x4b02)
  │    track 2 music ON → NEO logo fades in → fli\intro.fli → publisher
  │    logo → fli\fly.fli → cover (white flash, 320x400) → credits over
  │    fli\credz.fli → menu fade-in            (music keeps playing)
  │
  └─ MAIN MENU loop  (0x3e41)   [track 2 still playing]
       NEW GAME   → level state machine        (0x4b05)
       LOAD GAME  → 5-slot load screen          (0x4258)
       HIGHSCORES → high.txt over highscor.fli  (0x3f0c)
       MUSIC MENU → CD jukebox, tracks 2-8      (0x439a)
       QUIT       → stop music, restore, exit   (0x4d90)
```

## Boot (entry0 0x4705 .. 0x4791)

In order:

1. `0x10`: shrink the program's memory block (`int21 AH=4A`).
2. `0x3c7a`: load `high.txt` into `cs:[0x38d7]` (8 records x 22 bytes; also
   re-run on every highscores visit).
3. Drive-letter fixup: `int21 AH=19`, `+0x61` to a letter, patch the literal
   `x:` byte in every path string.
4. `0x39e0`: read `drive.nfo` (1 byte) into `cs:[0x4701]` and patch the drive
   letter of every title/menu asset path; `0x3a33`: read `fli.nfo` (1 byte)
   into `cs:[0x4702]` and patch the cutscene paths the same way. **A missing
   file exits straight to DOS** through `0x4d5f` (restore text mode,
   `int21 AH=4C`) with no message. Neither file ships on the CD; INSTALL.EXE
   creates them, so a raw file set needs them added by hand (1 byte, the
   lowercase drive letter).
5. `0x2839`: open `proto.cfg`; saved options live here (music on/off
   `cs:[0xf3c]`, volumes, joystick). A missing file falls back to defaults.
6. Joystick detect/calibrate (`0x2774`, `0x2828`), gated on `cs:[0xf3d]` /
   `cs:[0x2760]`.
7. `0x4512`: EMS check and alloc of the ~2.5 MB "Grafik Data" buffer (`0x2d38`
   probes EMS; on failure prints "2.5 MB of free EMS" and exits).
8. `0x3158`: build the 256-color palette (256 RGB triples from segment
   `0x513`).
9. `0x3a74`: delete a stale `message` file if one exists. Creation (`AH=3C`,
   `0x3a81`) happens around level launches, not here.
10. Display settle: `cx=0x46` loop on the vertical-retrace bit (port `0x3DA`
    bit 3), ~70 retraces.
11. `0x2e6f`: install the custom timer + keyboard ISRs (see below).
12. `int10 AX=0013`: set mode 13h (320x200x8, segment `0xA000`); load and
    decode the title assets via the loader `0x24`.

Before entering the title, entry0 saves the stack pointer to `cs:[0x34f5]`
(`0x4772`); the intro's key-skip longjmps back through it.

## Timing and input primitives

The timer ISR increments a free-running tick counter `cs:[0xf5c]` at the 70 Hz
retrace rate. Everything timed builds on it:

- `0x1068` zeroes the counter.
- `0x3024` (wait) spins until the counter reaches `cs:[0x3022]`, then zeroes
  it. Callers set `[0x3022]` and usually call `0x1068` first.
- `0x1047` zeroes the counter and waits for one tick (an alignment).

The `int 9` ISR does three things on every key-down: it increments the FLI
abort counter `cs:[0x2dea]`, sets the key-available flag `cs:[0x2dec]` with
the scancode in `cs:[0x2ded]`, and, if the skip gate `cs:[0x34f2]` is 1,
sets the skip flag `cs:[0x34f4]`.

The skip gate is baked 1 in the data image, so during the whole intro every
wait is key-skippable. The wait's skip exit (`0x3044`) clears the flag, closes
any open file, and jumps to the menu entry at `0x4a54`, which restores the
saved stack pointer, clears the gate, and runs the menu setup plus its 40-tick
fade-in. So one key anywhere in the intro aborts the entire script onto the
menu fade-in. The gate stays 0 afterwards; menus block on the key reader
`0x2e5c` (spins on `[0x2dec]`, returns the raw scancode, clears the flag).

The highscores FLI is **not** key-abortable: the highscore routine restores
the DOS `int 9` before playing it (the saved vector at `[0x2d63]`, init
`0xffff`) and reinstalls the custom ISR only after, so no key reaches
`[0x2dea]` during the movie. The between-levels cutscenes, by contrast, run
with the custom ISR live and their shared FLI player aborts when `[0x2dea]`
reads exactly 1.

### Palette fade (`0x2ec4`)

One routine drives every fade. Parameters: `dl` = step count, `cs:[0x3022]` =
ticks per step, `ax` = color count, `si`/`di` = from/to palette offsets in the
workspace segment `[0x33a8]`, `bp` = scratch offset (always `0x600`), `bx` =
first DAC index. Each step interpolates into the scratch, waits, and uploads
through the DAC writer `0x230` (`si` = source offset, `di` = first index,
`cx` = colors). Fade duration = `dl * [0x3022]` ticks.

## Title / intro (entry0 0x47db .. 0x4b02)

A scripted sequence over the title music. Stills blit from preloaded segments;
their palettes load into the fade workspace (slot 0 = target, slot `0x300` =
black). Durations:

| #   | Beat                                           | Ticks              |
| --- | ---------------------------------------------- | ------------------ |
| 1   | blit `neo.bdy` (DAC black), start CD track 2   |                    |
| 2   | hold                                           | 220                |
| 3   | fade in (110 steps x 2)                        | 220                |
| 4   | hold                                           | 350                |
| 5   | `fli\intro.fli` (31 frames x 3)                | 93                 |
| 6   | hold                                           | 230                |
| 7   | fade out (110 x 2)                             | 220                |
| 8   | blit `surplogo.bdy`; fade in (110 x 1)         | 110                |
| 9   | hold                                           | 200                |
| 10  | fade out (110 x 1)                             | 110                |
| 11  | mode 13h reset; `fli\fly.fli` (110 frames x 2) | 220                |
| 12  | cover reveal (below)                           | ~4 (machine-bound) |
| 13  | hold                                           | 180                |
| 14  | fade out, DAC indices 1..255 (90 x 1)          | 90                 |
| 15  | credits (below)                                | 6 x 40 frames x 8  |
| 16  | menu setup; fade in (40 x 1)                   | 40                 |

### Cover reveal (320x400)

After fly.fli the screen clears, slot 0 gets `cover3.pal`, slot `0x300` gets
all-white (`0x3f`), and the white uploads to the DAC: the screen flashes
white. Behind it: vsync + sequencer unchain (`0x1b5`), CRTC register 9 `&=
0x60` (scan doubling off, 400 visible lines), then 4 planes x `0x7d00` bytes
copy from segments `[0x34ea..0x34f0]`: 320x400, the top 400 rows of the
478-row BDY. The plane buffers free (`0x3be6`), one tick aligns (`0x1047`),
and `cover3.pal` uploads in one go (the upload at `0x4bfa`: 255 colors from
index 0, so indices 0..254). The white lasts as long as the copy, a few
ticks. Quirk: the fade-out afterwards (`0x4c31`, `bx=1`) covers DAC 1..255,
skipping only index 0; cover3.pal never set index 255, so it fades the
leftover white down with everything else. The net result matches a plain
full-palette fade. The credits reset mode 13h, back to 200 lines.

### Credits (`0x460b`)

Setup per play (`0x45cc`): allocate and zero a 64000-byte text buffer
(`[0x33a6]`), set `[0x3022]=8`. The sequence is one text-free play of
`fli\credz.fli`, then five plays each preceded by drawing a text page into the
buffer (`0x45f5`).

Pages are 12 rows of exactly 20 characters, drawn from screen offset `0x500`
(x = 0, y = 4) with a 16-scanline row stride, font.raw glyphs. The records
live in CS, 22 bytes each: an `X` marker, 20 characters, `$`; the page
pointers (`cs:0x6e5`, `0x7ed`, `0x8f5`, `0x9fd`, `0xb05`) point one past the
marker. Layout is authored into the padding: headers sit one cell in
(`" PROGRAM:"`), names are right-aligned to column 19
(`"         ERIK POJAR "`), and `"SPECIAL THANKS TO:  "` starts at x = 0. The
fifth page is 12 blank rows, the trailing linger.

The credits use their own FLI player (`0x3293`), different from the intro's:
it decodes each frame into a compose buffer (`[0x33a4]`), overlays the text
buffer (nonzero bytes win), blits the composite to the screen, and waits 8
ticks. It also plays **one frame short of the header count** (`dec cx` after
the header read), so each rotation shows 40 of credz.fli's 41 frames. It
checks `[0x2dec]` per frame and the caller checks it before each play: any
pending key aborts the credits, including a key pressed earlier in the intro,
since nothing before the menu consumes the flag.

## Main menu (0x3e41, labels drawn by 0x3df6)

Setup: re-set mode 13h, blit `back3.raw`, upload the menu palette (`0x230`),
draw five labels with the string drawer `0x3d89` at x=90: NEW GAME, LOAD GAME,
HIGHSCORES, MUSIC MENU, QUIT (rows y = 60/76/92/108/124).

Loop `0x3e41`:

- Draw the `>` cursor (glyph `0x3e`, an orange triangle in font.raw) at the
  current item, x=70, via `0x3d03`; block on the key reader `0x2e5c`.
- `0x50` Down / `0x48` Up: move by `0x1400` (16 scanlines), wrapping. The
  erase (`0x3dab`) restores the 16x16 cell from `back3.raw`.
- `0x1C` Enter: dispatch on the cursor offset (`cs:[0x3da5]`).
- **Every other scancode is ignored, Esc included.** Quitting is the QUIT
  item.

| Item       | Offset | Target | Action                                              |
| ---------- | ------ | ------ | --------------------------------------------------- |
| NEW GAME   | 0x4b46 | 0x4b05 | level state machine (below)                         |
| LOAD GAME  | 0x5f46 | 0x4258 | 5-slot load screen, then resume the saved level     |
| HIGHSCORES | 0x7346 | 0x3f0c | highscores screen (below)                           |
| MUSIC MENU | 0x8746 | 0x439a | CD jukebox (below)                                  |
| QUIT       | 0x9b46 | 0x4d90 | stop music, restore ISRs + text mode, `int21 AH=4C` |

### Text drawing (`0x3d03` glyph, `0x3d89` string)

`di` = screen offset, `es` = the target segment (`0xA000` or an offscreen
buffer), `ds` = the font segment. Strings read from **CS** (`cs:[si]`),
`$`-terminated. Glyph index = `char - 0x20`; the font sheet is 320 px wide, 20
glyphs of 16x15 per band, bands every 15 scanlines from y = 16; pixel value 0
is transparent; `di += 16` per cell. Details in `reference/formats/font.md`.

Two fonts load at boot: `font.raw` (`[0x3837]`: menu, credits, name entry)
and `font2.raw` (`[0x3845]`: the highscore table).

## NEW GAME: level state machine (0x4b05)

A loop driven by `cs:[0x46ff]` (the last level's outcome) and `cs:[0x4703]`
(current level 1-8). After each level, the WAD writes its result to the
`message` file; START.EXE reads it and branches:

- **state 0 / 1 (continue)**: stop the menu's CD audio, `call 0x3abe` to
  `EXEC` `level_N.wad`; the level plays its own track. On level 8, go to the
  ending.
- **state 2**: quit to DOS (`0x4d90`).
- **state 5 (out of lives)**: the game-over path, not a win. The respawn
  handlers write status 5 only when lives hit 0; the win path is status 0
  on level 8. Stop music, play `fli\go2.fli`, compare the score in `eax`
  against `cs:[0xf2f]`, enter a highscore via `0x3f0c`, return to the menu
  (and restart track 2).
- **states 3 / 4**: level error exit to DOS (3) and NEW GAME chain restart
  (4).
- **level 8 (0x4c06)**: the ending sequence (loads via `0x24`).

Before each level, `0x3b0a` plays the inter-level cutscene
`cutscene_table[level]`: a **32-byte-entry table at `0x36f4`** (`shl bx,5;
add bx,0x36f4`) indexed by `cs:[0x4703]`. The seven entries are `canyon`,
`space1`, `waldende`, `space2`, `tend`, `space3`, `lava`, with a per-level
branch at `0x3b3c`: the themed levels (1/3/5/7 → canyon/waldende/tend/lava) and
the race transitions (space1-3).

## HIGHSCORES (0x3f0c)

The routine first compares the incoming score (`cs:[0xf33]`) against the
lowest entry; from the menu it never qualifies and the view path at `0x4068`
runs.

View path: play `fli\highscor.fli` once at 4 ticks/frame (`[0x3022]=4`,
intro player), freezing on its last frame as the backdrop. Then the eight
`high.txt` records (13-char dot-padded name, space, 6-digit score = 20 chars)
fly in one at a time, drawn with **font2.raw** at x = 0, rows y = 65 + 16k.
At the end the screen blocks on `0x2e5c`; a key pressed during the fly-in
stays pending and exits immediately once the table settles.

Per entry (`0x2670`): the current screen snapshots into `[0x33a4]`, the
entry's line draws into a zeroed text buffer `[0x33a6]` at its final position,
and 25 composite steps run, counter `cs:[0x2643]` 960 → 0 by 40. The helper
(`0x2646`) computes a sampling window of `2*160*1000/(1000-c)` by
`2*100*1000/(1000-c)`, which works out to a zoom of `25/(step+1)` about the
screen center (160, 100): top-half rows fly down from above, bottom-half rows
rise from below. The row blitter (`0x141e`, horizontal offsets patched by
`0x1365`) samples the text buffer at the zoomed coordinates and falls back to
the snapshot where the text is 0; only rows 0..194 composite. The step loop
has **no timer wait**: the pace is machine-bound, and since every step does
the same fixed work (195 rows x 320 px), it is uniform.

Enter-name path (a won game): `"  CONGRATULATIONS=  "` at y = 65 and
`"  ENTER YOUR NAME   "` at y = 82 (font.raw, full-width padded strings at
x = 0), the name field at x = 48, y = 110. Up to 13 characters, A-Z only
(lowercase is uppercased), backspace edits, Enter or Esc confirms; the buffer
(`cs:[0x5cf]`) is `.`-padded.

## MUSIC MENU (0x439a)

A 7-entry jukebox (MUSIC 1..7, track N+1) with the same cursor/key loop but
its own layout: labels at **x = 120**, rows y = 46..142, cursor at **x = 80**.
Enter sets `cs:[0xf3b]` and calls the play-track wrapper `0x2b5` (stop, then
play). Esc (scancode 1) returns to the menu, which redraws and resets its
cursor to NEW GAME. The OST is CD-DA: track 1 is data, tracks 2-8 are the
seven songs.

## LOAD GAME (0x4258)

A 5-slot screen (GAME 1..5), same cursor/key loop; loads a save and resumes
at its level. The save file is the level WAD's `.psg` (the in-game menu
writes it); START.EXE reads only its first byte (the level number) to pick
which WAD to `EXEC`. The cross-tool handover is the `f:message` file: a
mode byte plus `{status, score:4, lives:1, bombs:1, weapons:4}`.

## Audio

- **Music = CD-DA via MSCDEX.** A separate driver segment (`0x4dc`) wrapped by
  three thunks, all gated on the music-enabled flag `cs:[0xf3c]`:
  - `0x265` **start music**: init + play the track in `cs:[0xf3b]`.
  - `0x2b5` **play track**: stop current, then play `cs:[0xf3b]` (the jukebox).
  - `0x2f0` **stop music**.
  - Driver ops: `0x4dc:0x6a` MSCDEX init (`int2F AX=1500`), `0x4dc:0x2e6` play
    (`AX=1510` device request), `0x4dc:0x32d` stop, `0x4dc:0x28c` track info.

  Track 2 (the title theme) starts at the top of the intro's opening black
  hold and plays through the menu; the jukebox selects any of tracks 2-8;
  launching a level stops the front-end's audio so the level WAD can play its
  own track.
- **No SoundBlaster in START.EXE.** The only IO ports it touches are keyboard
  (`0x60`), PIC (`0xa1`), and VGA (`0x3c0`..`0x3da`). The DSP / DMA-buffer
  strings are dead leftovers shared with the level engine, with zero references
  here. Sample (SMP) effects are entirely the in-game level WADs' job.

## The `x:` path convention

Path strings are stored with a literal `x:` prefix (`x:cover3.bdy`,
`x:fli\intro.fli`). At startup the code reads the current DOS drive
(`int21 AH=19`), turns it into a letter (`+0x61`), and overwrites the `x`
byte; `drive.nfo` and `fli.nfo` then override the asset and cutscene paths
respectively, so the assets can live on a different drive (the CD) than the
EXE.

## Assets (table at 0x3684 / 0x3855)

- `level_1.wad` .. `level_7.wad` (`0x3684`, 16-byte entries): the level engines,
  launched by `0x3abe`.
- Inter-level cutscenes (`0x36f6`): `canyon`, `space1`, `waldende`, `space2`,
  `tend`, `space3`, `lava` (`fli\*.fli`).
- `x:pvessel.raw` (`0x3855`): player-vessel image.
- `fli\intro.fli`, `fli\fly.fli`, `fli\go2.fli`: intro and start-of-game
  cutscenes. `fli\highscor.fli`, `fli\credz.fli`: the highscore and credits
  screens.
- `cover3.bdy`/`.pal`, `neo.bdy`, `surplogo.bdy`: title and logo images.
- `font.raw`, `font2.raw`, `back3.raw`: the two fonts and the menu background.
- Data files: `proto.cfg` (options), `high.txt` (highscores, loaded to
  `cs:[0x38d7]`), `drive.nfo` / `fli.nfo` (1-byte drive letters, required;
  see boot), `message` (level-handoff IPC: deleted at boot, created by
  `0x3a81`, written by the level WAD, read back to drive the state machine).

## Level launch contract (0x3abe)

```
dec bx; shl bx, 4; add bx, 0x3684   ; table[level-1], 16 bytes/entry
mov dx, bx                          ; DX = "level_N.wad" path
mov si, 0x2acd                      ; SI = command tail = drive letter
call 0x2adc                         ; switch to game drive, EXEC, switch back
```

`0x2adc` switches to the game drive, EXECs (`int21 AH=4B00`), then reads the
exit code (`AH=4D`), which feeds the state machine above. The launched WAD
gets the drive letter as its command tail.

The table stores `level_1.wad` lowercase; the disc has `LEVEL_1.WAD` (DOS is
case-insensitive). So the port owns level→path resolution and the menu labels
in Rust; this table and the `0x030f` `$`-delimited label table are
reverse-engineering reference, not runtime data. Only blobs that are painful to
transcribe (the menu palette) are read at runtime, via the `start_exe` decoder.

## Subsystem primitives

| Routine                     | Address                                         | Role                                                                                     |
| --------------------------- | ----------------------------------------------- | ---------------------------------------------------------------------------------------- |
| DAC palette upload          | `0x230`                                         | `cx` colors from `ds:si` to index `di`, ports `0x3C8`/`0x3C9`                            |
| Palette fade                | `0x2ec4`                                        | `dl` steps x `[0x3022]` ticks; `si`→`di` palettes, `bx` first index                      |
| Tick counter                | `[0xf5c]`                                       | 70 Hz, incremented by the timer ISR                                                      |
| Zero counter / wait / align | `0x1068` / `0x3024` / `0x1047`                  | wait spins to `[0x3022]` ticks                                                           |
| Image blit                  | `0x2f04`                                        | copy a 64000-byte image to `0xA000`                                                      |
| FLI open/play               | `0x2f31` / `0x31fd`                             | streams to `0xA000`, full header frame count, `[0x3022]` ticks/frame                     |
| Credits FLI play            | `0x3293`                                        | compose buffer + text overlay, header count − 1 frames                                   |
| Highscore entry zoom        | `0x2670`                                        | 25 steps about the screen center; `0x2646` params, `0x141e` row blit, `0x1365` x-offsets |
| Glyph / string draw         | `0x3d03` / `0x3d89`                             | string from `cs:[si]`, `$`-terminated; see font.md                                       |
| Cursor move                 | `0x3dab`                                        | restore the 16x16 background cell, redraw `>`                                            |
| Key read (blocking)         | `0x2e5c`                                        | scancode from the `int 9` queue                                                          |
| Intro skip exit             | `0x3044`                                        | on `[0x34f4]`: longjmp to the menu entry `0x4a54` (SP from `[0x34f5]`)                   |
| Unchain + vsync             | `0x1b5`                                         | the cover's 400-line mode setup                                                          |
| ISR install/restore         | `0x2e6f` / `0x4d90`                             | timer + keyboard handlers                                                                |
| File loader                 | `0x24`                                          | open/read/close an asset into a segment                                                  |
| Silent exit                 | `0x4d5f`                                        | restore text mode, `int21 AH=4C` (missing nfo files land here)                           |
| CD music                    | thunks `0x265`/`0x2b5`/`0x2f0` → driver `0x4dc` | MSCDEX play/stop/info                                                                    |

## Port mapping

The front-end lives in `crates/game/src/scene/`: `intro.rs` (the script,
credits, cover, skip), `menu.rs` + `music.rs` over `list_menu.rs` (per-screen
`MenuLayout`), `highscores.rs` (FLI, fly-in zoom, name records). The timing
primitives are `fade.rs` and `flic_player.rs`; the platform steps scenes at
70 Hz with a catch-up fixed timestep, and the renderer fits each scene's
framebuffer into 4:3, which is how the cover's 320x400 beats display. The
highscore fly-in's absolute pace is the one number with no authored source
(machine-bound in the original); the port uses 125 ms per step, calibrated to
DOSBox at its default cycles.

## The launcher IPC the port replaces

The original passes level outcomes back through a `message` file the level WAD
writes and START.EXE reads (`cs:[0x46ff]` state, `cs:[0x4703]` next level, the
exit code `bl` compared at `0x2b16`). The port is one process: it carries the
level-to-level state in memory as a `Handoff` and drives outcomes from the level
scene's flow, so it never reads or writes that file. The exact `message` byte
layout is level-WAD-side and the port does not need it. The save format itself is
decoded in [savegame.md](savegame.md); the LOAD GAME handoff is above.

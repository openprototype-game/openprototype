# START.EXE: front-end / launcher

The menu front-end and level launcher: it plays the intro, runs the menu, and
`EXEC`s `level_N.wad` to play a level. Each `level_N.wad` is a standalone DOS
program (the engine plus that level's data), so there is no separate engine
binary. The `proto13.exe` string is a dead dev leftover (zero code references).

This file is both the structural reference and the runtime flow; it is the
implementation roadmap for the port's front-end.

Addresses are r2 flat vaddr (`file = vaddr + 0x200`, the MZ header). Traced in
r2 6.1.4.

## Format

- DOS MZ, 16-bit real mode, x86. 22064 bytes, header 512 bytes (`0x200`).
- Entry `CS:IP = 0000:4705` (r2 `entry0`).
- No mouse (no `int 0x33`): keyboard-driven, through a custom `int 9` ISR.
- The CD-audio music driver is a separate code segment (`0x4dc`), reached
  through thunks at `0x265`–`0x308`.

## Process model

Front-end and level runtimes are separate programs. START.EXE starts the title
music, plays the intro, runs the menu, and on NEW GAME drives a level-by-level
state machine that `EXEC`s each WAD and reads its outcome back from a shared
`message` file. The port can mirror this as a front-end state machine plus a
level runtime; it need not be multiple processes.

## Top-level flow

```
entry0 (0x4705)
  │
  ├─ BOOT
  │    mem-shrink · drive-letter fixup · load proto.cfg · joystick detect
  │    EMS check + alloc the ~2.5 MB buffer · build palette · install ISRs
  │    set VGA mode 13h · load title assets
  │
  ├─ TITLE / INTRO  (0x47db .. 0x4a68)
  │    track 2 music ON  →  cover image → palette fade-in
  │      → fli\intro.fli → fade → logo(s) → fade → fli\fly.fli
  │      → credits → fade into the menu        (music keeps playing)
  │
  └─ MAIN MENU loop  (0x3e41)   [track 2 still playing]
       NEW GAME   → level state machine        (0x4b05)
       LOAD GAME  → 5-slot load screen          (0x4258)
       HIGHSCORES → high.txt over highscor.fli  (0x3f0c)
       MUSIC MENU → CD jukebox, tracks 2-8      (0x439a)
       QUIT       → stop music, restore, exit   (0x4d90)
```

## Boot (entry0 0x4705 .. 0x4791)

In order. Roles marked *(traced)* were confirmed by disassembling the routine;
the rest are named from the call site.

1. `0x10` — shrink the program's memory block (`int21 AH=4A`). *(traced)*
2. Drive-letter fixup: `int21 AH=19`, `+0x61` to a letter, patch the literal
   `x:` byte in every path string. *(traced)*
3. `0x2839` — open `proto.cfg` (`AH=3D`); saved options live here (music
   on/off `cs:[0xf3c]`, volumes, joystick). *(traced: the open)*
4. `0x39e0` — read `drive.nfo` (1 byte) into `cs:[0x4701]` and patch the `x:`
   drive letter of every title/menu asset path (`cover3.bdy/.pal`, `neo.*`,
   `surplogo.*`, `font.raw`, `font2.raw`, `back3.raw`), so the assets can live
   on a different drive than the EXE. `0x3a33`, `0x3c7a` not traced. *(traced:
   drive.nfo)*
5. Joystick detect/calibrate (`0x2774`, `0x2828`), gated on `cs:[0xf3d]` /
   `cs:[0x2760]`. *(gating traced)*
6. `0x4512` — EMS check and alloc of the ~2.5 MB "Grafik Data" buffer (`0x2d38`
   probes EMS; on failure prints "2.5 MB of free EMS" and exits). *(traced)*
7. `0x3158` — build the 256-colour palette (256 RGB triples from segment
   `0x513`). *(traced)*
8. `0x3a74` — create the `message` file (`AH=3C`), the level-handoff file.
   *(traced)*
9. Display settle: `cx=0x46` loop on the vertical-retrace bit (port `0x3DA`
   bit 3), ~70 retraces. *(traced)*
10. `0x2e6f` — install the custom timer + keyboard ISRs (the `int 9` handler
    fills the scancode queue at `cs:[0x2dec]`/`[0x2ded]`). *(named)*
11. `int10 AX=0013` — set mode 13h (320x200x8, segment `0xA000`); load and
    decode the title assets via the loader `0x24`. *(traced)*

## Title / intro (entry0 0x47db .. 0x4a68)

A scripted sequence of full-screen images, palette fades, and two FLI
cutscenes, with the title music playing throughout.

1. `cs:[0xf3b] = 2`, `call 0x265` — **start CD track 2** (title music; gated on
   the music-enabled flag). CD audio plays independently of the CPU, so it
   carries through the whole intro and the menu.
2. Blit the cover image (64000 bytes) to `0xA000`; fade the palette in; hold.
3. Play `fli\intro.fli`.
4. Fade out; blit the next logo; fade in; hold.
5. Re-set mode 13h; play `fli\fly.fli`.
6. Run the closing fades into the menu palette, then enter the menu.

Primitives: blit image `0x2f04`, fade `0x2ec4`, delay `0x1068`+`0x3024`, play
FLI `0x2f31`(open)+`0x31fd`(play). A key during the intro skips ahead.

The images are `cover3.bdy` (title), `neo.bdy` (NEO Software logo), and
`surplogo.bdy` (publisher logo) — path strings at `cs:[0x37d4]` / `[0x37f2]` /
`[0x380a]`, loaded into the segments the blits read. The credits text lives at
image offset `0x600` and is shown in this sequence. **TBD:** the exact
credits-overlay step.

## Main menu (0x3e41, set up by entry0 0x4a69 .. 0x4b02)

Setup: `0x413e`, re-set mode 13h, blit `back3.raw`, upload the menu palette
(`0x230`), draw five labels with the string blitter `0x3d89` at x=90: NEW GAME,
LOAD GAME, HIGHSCORES, MUSIC MENU, QUIT.

Loop `0x3e41`:

- Redraw the `>` cursor (glyph `0x3e`) at the current item via `0x3d03`, then
  block on the key reader `0x2e5c` (raw scancode).
- `0x50` Down / `0x48` Up: move the cursor by `0x1400` (16 scanlines), wrapping.
  Items sit at framebuffer offsets `0x4b46`, `0x5f46`, `0x7346`, `0x8746`,
  `0x9b46` (x≈70, y = 60/76/92/108/124).
- `0x1C` Enter: dispatch on the current offset (`cs:[0x3da5]`):

| Item | Offset | Target | Action |
|------|--------|--------|--------|
| NEW GAME | 0x4b46 | 0x4b05 | level state machine (below) |
| LOAD GAME | 0x5f46 | 0x4258 | 5-slot load screen, then resume the saved level |
| HIGHSCORES | 0x7346 | 0x3f0c | draw `high.txt` over `highscor.fli`, wait for a key |
| MUSIC MENU | 0x8746 | 0x439a | CD jukebox (below) |
| QUIT | 0x9b46 | 0x4d90 | stop music, restore ISRs + text mode, `int21 AH=4C` |

### Text drawing (`0x3d03` glyph, `0x3d89` string)

`di` = screen offset, `es = 0xA000`. Glyph index = `char - 0x20`. The font
(`font.raw`, 320 px wide) is **20 glyphs/row, each 16x15, glyph area at y=16**
(source base `si = 0x1400`). Pixel value 0 is transparent. `di += 16` per cell.

### Key input (`0x2e5c`, blocking)

Spins until `cs:[0x2dec] != 0`, returns the scancode from `cs:[0x2ded]`, clears
the flag. Written by the custom `int 9` keyboard ISR. Raw scancode, not ASCII.

## NEW GAME: level state machine (0x4b05)

A loop driven by `cs:[0x46ff]` (the last level's outcome) and `cs:[0x4703]`
(current level 1-8). After each level, the WAD writes its result to the
`message` file; START.EXE reads it and branches:

- **state 0 / 1 (continue)**: stop the menu's CD audio, `call 0x3abe` to
  `EXEC` `level_N.wad`; the level plays its own track. On level 8, go to the
  ending.
- **state 2**: quit to DOS (`0x4d90`).
- **state 5 (won)**: stop music, play `fli\go2.fli`, compare the score in
  `eax` against `cs:[0xf2f]`, enter a highscore via `0x3f0c`, return to the
  menu (and restart track 2).
- **level 8 (0x4c06)**: the ending sequence (loads via `0x24`).

Before each level, `0x3b0a` plays the inter-level cutscene
`cutscene_table[level]` — a **32-byte-entry table at `0x36f4`** (`shl bx,5;
add bx,0x36f4`) indexed by `cs:[0x4703]`. The seven entries are `canyon`,
`space1`, `waldende`, `space2`, `tend`, `space3`, `lava`, with a per-level
branch at `0x3b3c`: the themed levels (1/3/5/7 → canyon/waldende/tend/lava) and
the race transitions (space1-3).

## Submenus

- **MUSIC MENU (0x439a)** — a 7-entry jukebox (MUSIC 1..7), same cursor/key
  loop. Enter sets `cs:[0xf3b]` to the track (MUSIC N → CD track N+1) and calls
  the play-track wrapper `0x2b5`. The OST is CD-DA: track 1 is data, tracks 2-8
  are the seven songs.
- **HIGHSCORES (0x3f0c)** — loads `high.txt` and draws it over
  `fli\highscor.fli`; also the new-entry path from the win sequence. `high.txt`
  is **8 entries × 22 bytes**: a 13-char name (`.`-padded), a space, a 6-digit
  decimal score, then `$\n`. The shipped defaults are the dev team (`ERIK`
  010000 … `KATYA` 003000).
- **LOAD GAME (0x4258)** — a 5-slot screen (GAME 1..5), same cursor/key loop;
  loads a save and resumes at its level. **TBD:** save-file format and who
  writes it (likely the level WAD).
- **QUIT (0x4d90)** — stop music (`0x2f0`), restore the saved interrupt vectors
  (`AH=25`), return to text mode, `int21 AH=4C`.

## Audio

- **Music = CD-DA via MSCDEX.** A separate driver segment (`0x4dc`) wrapped by
  three thunks, all gated on the music-enabled flag `cs:[0xf3c]`:
  - `0x265` **start music** — init + play the track in `cs:[0xf3b]`.
  - `0x2b5` **play track** — stop current, then play `cs:[0xf3b]` (the jukebox).
  - `0x2f0` **stop music**.
  - Driver ops: `0x4dc:0x6a` MSCDEX init (`int2F AX=1500`), `0x4dc:0x2e6` play
    (`AX=1510` device request), `0x4dc:0x32d` stop, `0x4dc:0x28c` track info.

  Track 2 (the title theme) plays from the intro through the menu; the jukebox
  selects any of tracks 2-8; launching a level stops the front-end's audio so
  the level WAD can play its own track.
- **No SoundBlaster in START.EXE.** The only IO ports it touches are keyboard
  (`0x60`), PIC (`0xa1`), and VGA (`0x3c0`–`0x3da`). The DSP / DMA-buffer
  strings are dead leftovers shared with the level engine, with zero references
  here. Sample (SMP) effects are entirely the in-game level WADs' job.

## The `x:` path convention

Path strings are stored with a literal `x:` prefix (`x:cover3.bdy`,
`x:fli\intro.fli`). At startup the code reads the current DOS drive
(`int21 AH=19`), turns it into a letter (`+0x61`), and overwrites the `x` byte.
So `x:` means "the drive the game runs from".

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
- `font.raw`, `font2.raw`, `back3.raw`: menu font and background.
- Data files: `proto.cfg` (options), `high.txt` (highscores), `fli.nfo` (FLI
  sequence info), `drive.nfo` (1-byte asset drive letter, read by `0x39e0`),
  `message` (level-handoff IPC: created by `0x3a74`, existence-checked by
  `0x39c7`, written by the level WAD, read back to drive the state machine).

## Level launch contract (0x3abe)

```
dec bx; shl bx, 4; add bx, 0x3684   ; table[level-1], 16 bytes/entry
mov dx, bx                          ; DX = "level_N.wad" path
mov si, 0x2acd                      ; SI = command tail = drive letter
call 0x2adc                         ; switch to game drive, EXEC, switch back
```

`0x2adc` switches to the game drive, EXECs (`int21 AH=4B00`), then reads the
exit code (`AH=4D`) — which feeds the state machine above. The launched WAD
gets the drive letter as its command tail.

The table stores `level_1.wad` lowercase; the disc has `LEVEL_1.WAD` (DOS is
case-insensitive). So the port owns level→path resolution and the menu labels
in Rust; this table and the `0x030f` `$`-delimited label table are
reverse-engineering reference, not runtime data. Only blobs that are painful to
transcribe (the menu palette) are read at runtime, via the `start_exe` decoder.

## Subsystem primitives

| Routine | Address | Role |
|---------|---------|------|
| DAC palette upload | `0x230` | 256 colours to ports `0x3C8`/`0x3C9` |
| Palette fade | `0x2ec4` | interpolate two palettes over N steps (`0x2ea2`/`0x3024`/`0x230`); N from `cs:[0x3022]` |
| Delay / wait | `0x1068` / `0x3024` | set a frame countdown, wait (vblank-paced, key-skippable) |
| Image blit | `0x2f04` | copy a 64000-byte image to `0xA000` |
| FLI open/play/decode | `0x2f31` / `0x31fd` / `0x2f66` | same FLI format we decode |
| Glyph / string draw | `0x3d03` / `0x3d89` | font.raw sheet, 16x15 cells, value 0 transparent |
| Cursor move | `0x3dab` | erase + redraw the `>` cursor |
| Key read (blocking) | `0x2e5c` | scancode from the `int 9` queue |
| ISR install/restore | `0x2e6f` / `0x4d90` | timer + keyboard handlers |
| File loader | `0x24` | open/read/close an asset into a segment |
| EMS alloc | `int 0x67` @ `0x2d57`, `0x4531`; check `0x4512` | the ~2.5 MB buffer |
| CD music | thunks `0x265`/`0x2b5`/`0x2f0` → driver `0x4dc` | MSCDEX play/stop/info |

## Port plan

The port already has the decoders this front-end needs: FLI, BDY, RAW, the SP
backgrounds, PAL, the menu font, the menu palette via `start_exe`, and the disc
reader for CD-DA tracks. So the front-end is mostly orchestration over existing
decoders plus a few new primitives.

Suggested build order:

1. **Video layer**: a 320x200 indexed framebuffer + palette, a present step,
   and the palette-fade primitive. Everything draws into this.
2. **Asset staging**: load and decode the title assets through the decoders.
3. **Intro player**: a data-driven step list (image → fade → FLI → … → menu),
   key-skippable, using the FLI decoder we have.
4. **Audio**: CD-DA playback (we already read the tracks off the image) — start
   track 2 for the title/menu, jukebox for the music menu, stop on launch.
5. **Menu state machine**: the five-item menu and cursor/key loop, then the
   jukebox (needs only CD playback), then highscores (`high.txt`) and load/save
   once their formats are traced.
6. **Level launch**: the port owns level→path resolution and the labels in
   Rust; "EXEC level_N.wad" becomes "run the level runtime for level N".

## Open / TBD

These remaining items are level-WAD-side (a separate binary) or need a sample
file the disc does not ship:

- The save-file format and who writes it (LOAD GAME; likely the level WAD; no
  save ships on the disc).
- The `message`-file content: which bytes the level WAD writes map to which
  `cs:[0x46ff]` state and the next `cs:[0x4703]` level.
- The level WAD exit-code values (the `bl` compared at `0x2b16`).
- The credits-overlay step and the role of `fli.nfo`.

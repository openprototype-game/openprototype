# START.EXE: front-end / launcher map

Status: first pass. Skeleton confirmed by disassembly (r2); inner routines
(menu draw, input loop, asset loader) not yet traced.

START.EXE is the menu front-end. The actual gameplay engine is a separate
binary, `proto13.exe` (VGA mode 13h), which is **not present** in this asset
set. START.EXE sets up video, shows the menu/intro, and launches the engine
or a level.

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
| Program launcher | `fcn.00002aff`: builds command tail, `AX=4B00; int 0x21` @ 0x2b31, then `AH=4D` exit code | disasm |
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

## Open threads (next digs)

- Which routine loads `cover3.bdy` / `back3.raw` / `font.raw` and blits them.
- The menu state machine: how items in the 0x050f table map to actions, and
  the keyboard input loop.
- How the launcher chooses `proto13.exe` vs a `level_N.wad`, and what command
  tail it passes (this is the level entry point).
- The other `int 0x10` sites (0x47c4, 0x4902, 0x4a6f, ...): palette/page
  setup vs. additional mode changes.

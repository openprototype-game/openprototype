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

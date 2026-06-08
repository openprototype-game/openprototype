# Combat: weapons, firing, damage

The in-level weapon and damage system, the same in every `LEVEL_N.WAD`. Traced
from `LEVEL_1.WAD` in objdump (i8086) and cross-checked against gameplay video.

Two address kinds appear here. **Code** is r2/objdump flat vaddr
(`file = vaddr + 0x200`, the MZ header). **Data** is a `cs:` segment offset
(`cs:[X]`, where `file = X + 0x29F0`); these are the live game-state variables.
Entity storage is covered separately in [entity-buffers.md](entity-buffers.md).

## Weapon roster

Five weapons: a minigun plus four leveled "secondaries". The minigun is the
always-available default; it has no level and no bar. Each secondary has a 0-4
charge level shown as one of the four HUD bars.

| `cs:` var | meaning |
|-----------|---------|
| `0xcb7` | **selected slot**, 1-4. What the switch marks; never names the minigun. |
| `0xcb9 / 0xcbb / 0xcbd / 0xcbf` | the four **levels**, stored `0,8,16,24,32` (logical 0-4; the `×8` is bar-fill pixels). |
| `0xcb5` | **firing weapon**, 0-4 (`0` = minigun). **Derived, never set by input.** |

`selected` and `firing` are distinct: `selected` is what you picked, `firing` is
what actually shoots (and what damage and death drain).

## Controls

The keyboard ISR (`0x2cc0`) decodes each scancode to `bx` (7-bit code, `+0x80`
for E0-extended keys) and `ah` (1=down/0=up), then writes a key-state table at
base `cs:0x3ae` (`cs:[bx+0x3ae]`) for every key, plus gated special-cases for the
cursor keys and right-hand modifiers. The per-frame poll `0xb0d8` copies these
into the action flags `cs:0x8153..0x8159`.

| Action | Flag | Key byte | Key(s) |
|--------|------|----------|--------|
| Fire (held, auto-repeat) | `0x8157` | `0x3cb` | **Ctrl** — `0x1d` Left (always), `0x9d` Right (gated `cs:0x4b4`); joystick button bit `0x10` |
| Switch weapon (edge) | `0x8158` | `0x3d8` | **Shift** — `0x2a` Left (always), `0x36` Right (gated `cs:0x4b3`); joystick bit `0x20` |
| Smart bomb (edge) | `0x8159` | `0x3e7` | **Space** (`0x39`); joystick 2nd button bit `0x40` (2-button pads) |
| Move U/L/R/D | `0x8155/54/53/56` | `0x3f6/f9/fb/fe` | **arrow keys or numeric keypad** (`0x48/4b/4d/50`) |

The table base `0x3ae` is chosen so the numeric-keypad scancodes land exactly on
the four movement bytes; the cursor arrows reach the same bytes through the
extended-key special-cases (gated `cs:0x4b6`). Joystick is read via `0x70d5`
(port `0x201`), button bitmask in `dl`.

## Firing

One fire button. Held fire (`0x8157`) auto-repeats on a cooldown (`cs:0xcc7`
counts to rate `cs:0xcc8`; minigun rate = 6). On a due shot the dispatch `0x981d`
branches on `firing` (`cs:0xcb5`):

| `firing` | shot |
|----------|------|
| `0` minigun | two barrels |
| `1` weapon 1 | three shots, +2 more at max level (`0x20`) |
| `2` weapon 2 | single stream |
| `3` weapon 3 | spread; projectile count scales with level at thresholds `8/16/24/32` (flags `cs:0xcda-0xcdd`) |
| `4` weapon 4 | the gs-segment special (counter `cs:0x267f`) |

The exact per-weapon patterns and the four secondaries' identities are not yet
fully decoded.

### Firing-weapon resolve

`firing` is re-derived each frame by the resolve (`0xac62`):

```
firing = (level[selected] >= 8) ? selected : minigun(0)
```

You fire your selected secondary while it has charge; the minigun fills in when
the selected slot is empty. The resolve is **gated by fire-held** (`0xac59`:
`cmp cs:0x8157,1; jne resolve; ret`, single call site from `0xb36e`), so `firing`
is frozen while fire is held and re-resolves the instant you release. On any
change it triggers the blob (HUD weapon icon) fade-in (`0xaa83`).

This matches gameplay: holding fire while orbs charge the selected slot keeps the
firing weapon (and the blob) on the minigun; releasing flips it to the now-charged
secondary. A manual Shift press produces the same flip.

### Switch

`0xaa08` / `0xaa44` (joystick `0x8158` / keyboard `0x3d8`, debounced
`0x7338`/`0x7337`): `inc cs:0xcb7; wrap 5→1; redraw selector (0xa9d8)`. Cycle
forward only; no reverse, no direct-select. Touches **only** `selected` — `firing`
catches up via the resolve.

## Charge and pickups

Pickups are entities in buffer C ([entity-buffers.md](entity-buffers.md)),
identified by health sentinels and handled in C's update loop:

- **Charge orb** (`0xd5f3`): `level[selected] += 1`, cap 4. One orb type; it
  always charges whatever is currently selected.
- **Smart-bomb pickup** (`0xd698`): `inc cs:0xcc3`, cap 3.
- **Extra-life pickup** (`0xd6c9`): `inc cs:0x2669`, cap 9.
- **Invincibility pickup** (`0xd6ba`): sets the shield timer to 600 (below).

The cheat (`0xb0b0`, behind key-flags `cs:0x3c5`/`0x3d3`) sets all four levels to
`0x20` and the shield timer to 32000.

## Smart bomb

Space (`0x8159`, edge) → `0x97b7`: if count `cs:0xcc3` is zero, nothing; else
`dec cs:0xcc3`, flash the screen, and arm the area-damage timer `cs:0x2745`. When
that timer fires, `cs:0x2743` is set to 600 and every enemy in buffer C has it
subtracted from its health that frame (`0xd4fb`), then `0x2743` resets to 0. Not a
weapon (does not go through the fire dispatch).

## Damage model

The weapon bars double as the player's health: charge absorbs hits, and only a
hit at zero charge costs a life. Two collision passes run per frame from the main
loop (`0xe742` then `0xf5e7`: `call 0xc343` then `call 0xdae1`), both gated by the
invincibility timer `cs:0x266a` and the already-dying flag `cs:0x46b2`.

- **Enemy contact** (full-zero) — loop `0xd8e1` bbox-tests each enemy in buffer C
  against the player. On a hit, handler `0xdade`:
  - firing is a secondary (`cb5 != 0`): **zero `level[firing]` entirely**
    (`bx=(cb5-1)*2; mov word cs:[bx+0xcb9],0`), revert to minigun (`cs:0xcb5=0`,
    rate `cs:0xcc8=6`) — **no life lost**.
  - firing is the minigun (`cb5 == 0`): **death** (`cs:0x46b2=1; call 0xab23`)
    → lose a life, GET READY, respawn.
- **Bullet graze** (`-1` level) — loop `0xc343` bbox-tests each enemy projectile
  in buffer B against the player; on a hit, `level[firing] -= 8` (one level,
  `0xc2c0`), revert to minigun at 0, death on minigun.

So ramming an enemy ship zeroes the bar in one hit; getting clipped by a bullet
chips one level. Either drops you toward the minigun, and either kills once you
are already on it. The shield zeroing in one hit (not draining gradually) on body
contact is confirmed frame-by-frame against video (full bar → one hit → level 0 +
minigun, no life lost; next hit → life −1 + GET READY).

### Death and respawn

Death (`0xab23` path) decrements lives (`dec cs:0x2669`, `0x9b8c`); at 0 it is
game over. Otherwise it respawns the ship and **arms invincibility**.

### Invincibility (`cs:0x266a`)

A per-frame countdown (decremented at `0xb2b1`) that gates *all* collision damage.
Set to **300** (`0x12c`) on every respawn (`0x9ba2`, right after the lives `dec`),
so ~3 s of invulnerability after each GET READY (level start and each death). The
invincibility pickup sets 600; the cheat sets 32000. The visible shield sprite
around the ship is drawn while `cs:0x266a != 0` (checked at `0xb8d3`).

## Open / TBD

- The four secondaries' identities and exact fire patterns (weapon 3 = spread,
  weapon 4 = the gs-segment special; the per-weapon projectile dispatch in
  `0x981d` is only partly decoded).
- New-game starting values for lives and weapon levels (not set in a level WAD's
  own paths; likely initialized by `START.EXE` and carried across level WADs).
- The bullet-graze list (buffer B) vs body-contact list (buffer C) is proven from
  the spawn side (see [entity-buffers.md](entity-buffers.md)); whether *every*
  enemy class body-contacts for the full zero (vs some special types at `0xdc9c`
  that set a behavior pointer and skip damage) is not exhaustively enumerated.

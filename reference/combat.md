# Combat: weapons, firing, damage

The in-level weapon and damage system, the same in every `LEVEL_N.WAD`. Traced
from `LEVEL_1.WAD` in objdump (i8086) and cross-checked against gameplay video.
The port code is `crates/game/src/{combat,shots,ship}.rs`.

Two address kinds appear here. **Code** is r2/objdump flat vaddr
(`file = vaddr + 0x200`, the MZ header). **Data** is a `cs:` segment offset
(`cs:[X]`, where `file = X + 0x29F0`); these are the live game-state variables.
Entity storage is in [entity-buffers.md](entity-buffers.md).

## Weapon roster

Five weapons: the chaingun plus four leveled secondaries. The chaingun is the
always-available default; it has no level and no bar. Each secondary has a 0..4
charge level shown as one of the four HUD bars.

| `cs:` var | Meaning |
|-----------|---------|
| `0xcb7`   | **selected slot**, 1..4 (the four secondaries; never names the chaingun) |
| `0xcb9 / 0xcbb / 0xcbd / 0xcbf` | the four **levels**, stored `0,8,16,24,32` (logical 0..4; the `x8` is bar-fill pixels) |
| `0xcb5`   | **firing weapon**, 0..4 (`0` = chaingun). Derived, never set by input |

The four secondaries are weapon 1 Multishot, 2 Burning, 3 Plasma, 4 Missile.
`selected` and `firing` are distinct: `selected` is what you picked, `firing` is
what actually shoots (and what damage and death drain).

## Controls

The keyboard ISR (`0x2cc0`) decodes each scancode to `bx` (7-bit code, `+0x80`
for E0-extended keys) and `ah` (1 = down, 0 = up), then writes a key-state table
at base `cs:0x3ae` (`cs:[bx+0x3ae]`) for every key, plus gated special cases for
the cursor keys and right-hand modifiers. The per-frame poll `0xb0d8` copies
these into the action flags `cs:0x8153..0x8159`.

| Action | Flag | Key byte | Key(s) |
|--------|------|----------|--------|
| Fire (held, auto-repeat) | `0x8157` | `0x3cb` | Ctrl: `0x1d` Left (always), `0x9d` Right (gated `cs:0x4b4`); joystick button bit `0x10` |
| Switch weapon (edge)     | `0x8158` | `0x3d8` | Shift: `0x2a` Left (always), `0x36` Right (gated `cs:0x4b3`); joystick bit `0x20` |
| Smart bomb (edge)        | `0x8159` | `0x3e7` | Space (`0x39`); joystick 2nd button bit `0x40` |
| Move U/L/R/D             | `0x8155/54/53/56` | `0x3f6/f9/fb/fe` | arrow keys or numeric keypad (`0x48/4b/4d/50`) |

The table base `0x3ae` is chosen so the numeric-keypad scancodes land exactly on
the four movement bytes; the cursor arrows reach the same bytes through the
extended-key special cases (gated `cs:0x4b6`). Joystick is read via `0x70d5`
(port `0x201`), button bitmask in `dl`.

## Firing

One fire button. Held fire (`0x8157`) auto-repeats on a cooldown (`cs:0xcc7`
counts to rate `cs:0xcc8`; chaingun rate = 6). On a due shot the dispatch
`0x981d` branches on `firing` (`cs:0xcb5`):

| `firing`    | Shot |
|-------------|------|
| `0` chaingun  | two barrels |
| `1` Multishot | three shots, plus two more at max level (`0x20`) |
| `2` Burning   | a single beam, with extra spawn rows per charge level |
| `3` Plasma    | the satellite orbs (see below) |
| `4` Missile   | one homing missile, alternating spawn offset |

### Firing-weapon resolve

`firing` is re-derived each frame by the resolve (`0xac62`):

```
firing = (level[selected] >= 8) ? selected : chaingun(0)
```

You fire your selected secondary while it has charge; the chaingun fills in when
the selected slot is empty. The resolve is gated by fire-held (`0xac59`:
`cmp cs:0x8157,1; jne resolve; ret`, single call site from `0xb36e`), so
`firing` is frozen while fire is held and re-resolves the instant you release. On
any change it plays the weapon-switch sound and steps the HUD pod animation.

Holding fire while orbs charge the selected slot keeps the firing weapon on the
chaingun; releasing flips it to the now-charged secondary. A manual Shift press
produces the same flip.

### Switch

`0xaa08` / `0xaa44` (joystick `0x8158`, keyboard `0x3d8`, debounced
`0x7338`/`0x7337`): `inc cs:0xcb7; wrap 5 -> 1; redraw selector (0xa9d8)`. Cycle
forward only; no reverse, no direct select. It touches only `selected`; `firing`
catches up via the resolve.

### Plasma orbs

Plasma is the satellite orb weapon. Firing it deploys every charged orb at once;
the orbs trail the ship, bobbing on a 14-word wave, and each deployed orb fires
an instant bolt every tick (per-orb collision widths `[21, 22, 28, 28]`).
Releasing fire retracts the orbs and launches the last one forward as a slow orb
projectile. The deploy machine is paced by the stage byte `cs:0xcde`, the one
field a save preserves (see [savegame.md](savegame.md)).

### Missile homing

Each fired missile locks the next live entity in a round-robin (`cs:0x267f`) and
steers toward it once per tick (`0xc114`). The turn is inverse-square weighted:
the closer the target, the harder the turn, renormalized to a constant 3 px per
step. The missile refaces its sprite to one of eight velocity octants and leaves
a trail. A point-blank target (zero weight) drops the lock and the missile flies
straight.

## Charge and pickups

Pickups are entities in buffer C ([entity-buffers.md](entity-buffers.md)),
identified by kind and handled in the body-contact pass:

- **Weapon upgrade** (`0xd5f3`): `level[selected] += 1`, cap 4. One type; it
  charges whatever is currently selected.
- **Smart-bomb pickup** (`0xd698`): `inc cs:0xcc3`, cap 3.
- **Extra-life pickup** (`0xd6c9`): `inc cs:0x2669`, cap 9.
- **Invincibility pickup** (`0xd6ba`): sets the shield timer to 600.

The cheat (`0xb0b0`, behind key-flags `cs:0x3c5`/`0x3d3`) sets all four levels to
`0x20` and the shield timer to 32000.

### Weapon-upgrade drop

Every Nth enemy kill converts the dying enemy into a weapon-upgrade pickup in
place rather than just removing it (`reap` in `combat.rs`, the original's update
loop). The conversion repoints the entity to the weapon-upgrade type at the
enemy's center, gives it the pickup's hitboxes, and marks it seen so an
off-screen kill's pickup culls immediately. A score milestone grants an extra
life every 10,000 points, capped at 9.

## Smart bomb

Space (`0x8159`, edge) calls `0x97b7`: if count `cs:0xcc3` is zero, nothing; else
`dec cs:0xcc3`, flash the screen, and arm the area-damage timer `cs:0x2745`.
When that timer fires, `cs:0x2743` is set to 600 and every enemy in buffer C has
it subtracted from its health that frame (`0xd4fb`), then `0x2743` resets to 0.
It also emits a cosmetic expanding ring of inert shots. The smart bomb is not a
weapon and does not go through the fire dispatch.

## Damage model

The weapon bars double as the player's health: charge absorbs hits, and only a
hit at zero charge costs a life. Two collision passes run per frame from the main
loop (`0xe742` then `0xf5e7`), both gated by the invincibility timer `cs:0x266a`
and the already-dying flag `cs:0x46b2`.

### Ship hit rects

The ship presents three hit rects for its current roll frame (`ship_rects`, file
`0xda25`). The original indexes a pointer table byte-granularly at `roll / 9`;
the roll counts in `0x12` steps, so that is `frame * 2` over a table that
duplicates each word. The pointed-at block holds three 4-byte boxes anchored at
the ship; a `0xff` offset pushes a box ~255 px away, disabling it with no
explicit check.

### The two passes

- **Body contact** (`0xdae1`) tests each enemy's three boxes in buffer C against
  the three ship rects. A pickup grants and is removed. A rammed enemy costs the
  ship a contact hit and dies in place unless it is an orbiter or boss
  (`ram_survivors`).
- **Bullet graze** (`0xc343`) tests each enemy projectile in buffer B against the
  ship rects.

On a hit while a secondary is firing, body contact **zeroes** `level[firing]`
entirely and reverts to the chaingun, no life lost; a graze drains one level
(`-8`). A hit while the chaingun is firing is **death**. So ramming an enemy
zeroes the bar in one hit, a bullet clips one level, and either kills once you
are already on the chaingun. This is confirmed frame by frame against video.

### Player shots vs enemies

The player-shot pass (`0xc328`, hit test `0xbf47`, damage `0xc0a4`) sizes each
shot as an AABB by its spawner and tests it against every live entity's three
boxes. The first overlap spends the shot's damage budget; overkill pierces (the
shot keeps the remainder and flies on). A kill pays the type's score (a dword in
its descriptor) and feeds the weapon-upgrade-drop counter.

### Death and respawn

Death (`0xab23`) freezes the level (the dying flag `cs:0x46b2`), runs the
explosion sequence, then decrements lives (`dec cs:0x2669`, `0x9b8c`) and
respawns through GET READY with invincibility armed. At zero lives the level
exits with game-over status 5, handed back to START.EXE (see
[start-exe.md](start-exe.md)).

### Invincibility (`cs:0x266a`)

A per-frame countdown (decremented at `0xb2b1`) that gates all collision damage.
Set on every respawn (right after the lives `dec`) to a per-level value: 300
(`0x12c`) in L1 and L5, 180 (`0xb4`) in L2/L3/L4/L6/L7, so roughly 3 s or 1.8 s
of invulnerability after each GET READY (level start and each death). The
invincibility pickup sets 600; the cheat sets 32000. The shield sprite around the
ship draws while `cs:0x266a != 0` (checked at `0xb8d3`).

The new-game starting lives and weapon levels are not set in a level WAD's own
paths; START.EXE seeds them and carries them across levels as the handoff
economy (see [start-exe.md](start-exe.md) and [savegame.md](savegame.md)).

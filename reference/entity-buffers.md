# Entity buffers: the level engine's object pools

How the in-level engine (`LEVEL_N.WAD`) stores moving objects: the player's
shots, enemies, enemy fire, pickups, and effects. Traced from `LEVEL_1.WAD` in
objdump. Underpins the damage model in [combat.md](combat.md); the AI side of
the entity record is in [enemy-ai.md](enemy-ai.md), and how the buffers
serialize into a save is in [savegame.md](savegame.md).

Addresses are r2/objdump flat vaddr (`file = vaddr + 0x200`); `cs:[X]` is a data
offset. Each buffer is a far segment whose first word (`seg:0x0`) is the live
entity count; records follow at `seg:0x2`, fixed stride.

## Allocation

Init `0x7e04` allocates ten segments (`call 0x2804`, size in paragraphs) and
zeroes each count. They pair into **five double-buffered logical buffers**: a
front and a back segment each, flipped by a generation index that cycles
`0 -> 2`.

| Buffer | Segments (`cs:`)  | Size (para) | Generation idx | Role                |
|--------|-------------------|------------:|----------------|---------------------|
| A      | `0x29e3 / 0x29e5` |      `0x64` | `cs:0xcc5`     | player shots        |
| B      | `0x29e7 / 0x29e9` |      `0x64` | `cs:0xcc5`     | enemy projectiles   |
| C      | `0x29f3 / 0x29f5` |      `0x64` | `cs:0xcc5`     | enemies + pickups   |
| D      | `0x29eb / 0x29ed` |      `0x64` | `cs:0xccd`     | fire staging        |
| E      | `0x29ef / 0x29f1` |     `0x190` | `cs:0xccf`     | effects             |

A, B, and C share one generation index (`cs:0xcc5`), so they flip together each
frame; D and E have their own. To use a buffer, code loads its segment via
`mov <seg-reg>, cs:[bx + <table>]` with `bx` set to the generation index.

## Buffers, proven by their spawn and update sides

- **A `0x29e3` = player shots.** A's move loop (`0xc14a`) holds the enemy buffer
  C in `gs` and runs a per-entity collision against it, spawns a trail effect
  into E every other frame (`0x365a` via `0xc0f6`), culls off-screen, and draws
  last so shots sit on top (`0xba0b`). This is the live pool the player's
  ordnance moves, hits, and trails from.
- **D `0x29eb` = fire staging.** The fire handler spawns new shots here
  (`0xb4ec` sets `ds = D` before the fire dispatch); the chaingun, the secondary
  weapons, and the smart-bomb wave all append into D. Each frame D is merged into
  A, so D only ever holds the current frame's new shots. The port collapses the
  two: it appends straight into the live shot list.
- **C `0x29f3` = enemies + pickups.** Its per-frame update loop `0xd4fb`
  subtracts accumulated damage from each entity's health `[si+0x18]`; on death or
  a sentinel value it routes to the pickup handlers (charge orb `0xd5f3`,
  smart-bomb `0xd698`, extra-life `0xd6c9`; sentinels like `0x8300`). C holds
  everything the player shoots and collects. Targeted by the homing missile and
  the body-contact collision (`0xd8e1`). Drawn at `0xb577`.
- **B `0x29e7` = enemy projectiles.** Spawned by the enemy-fire routine `0xdc35`,
  which reads each enemy in C and emits bullets per that enemy's fire pattern
  (`[si+0x1b]` indexing table `cs:0x46b3`). Tested by the bullet-graze collision
  (`0xc343`). On the shipped levels this pass is inert; the port drives enemy
  fire from the AI functions instead (see [enemy-ai.md](enemy-ai.md)).
- **E `0x29ef` (big) = effects.** Hit sparks (`0x34da`), shot trails (`0x365a`),
  explosions. The collision and move routines spawn into E as `fs`; drawn every
  frame at `0xb5aa`.

The C-to-B link is the proof of the ship/bullet split the damage model rests on:
enemies (C) fire bullets (B); body contact with C zeroes the weapon bar, a graze
from B chips one level.

## Record layouts

The three record shapes, as they live in memory and serialize into a save. All
fields are little-endian.

### Shot (A, B, D) — 16 bytes

| Offset | Field  | Width | Notes                          |
|--------|--------|-------|--------------------------------|
| `+0x0` | sprite | word  | sprite descriptor pointer       |
| `+0x2` | x      | word  | 12.4 fixed                      |
| `+0x4` | y      | word  | 12.4 fixed                      |
| `+0x6` | vx     | word  | 12.4 velocity                   |
| `+0x8` | vy     | word  | 12.4 velocity                   |
| `+0xA` | size   | word  | collision box, copied from the descriptor |
| `+0xE` | damage | word  | the hit consequence ignores it  |

The port models a player shot as a kind plus an octant rather than the raw
sprite pointer, so `size` and `damage` are re-derived at collision time and a
save leaves them zero.

### Entity (C) — 0x40 bytes

The full field table is in [enemy-ai.md](enemy-ai.md). The type descriptor a
record points at (`kind`) holds the rest sprite, the three hitboxes, and the
death-debris template (`+0x14`); the hitboxes and debris are copied into the
record at spawn. Some AIs patch the shared descriptor in place per step, so the
port carries those patches as a `debris_override` that wins at death.

### Effect (E) — 16 bytes

| Offset | Field  | Width | Notes                                   |
|--------|--------|-------|-----------------------------------------|
| `+0x0` | sprite | word  | descriptor pointer; advances by `step`   |
| `+0x2` | x      | word  | pixel position (not 12.4)                |
| `+0x4` | y      | word  | pixel position                           |
| `+0x6` | frames | byte  | remaining frames; drops at zero          |
| `+0x7` | rate   | byte  | sub-steps per frame                      |
| `+0x8` | step   | word  | added to `sprite` per frame (always 8)   |
| `+0xA` | phase  | byte  | sub-steps into the current frame         |
| `+0xB` | delay  | word  | start delay; no draw or animate until 0  |

The `delay` staggers explosion bursts: an effect with a delay neither draws nor
animates until it burns off.

## Per-frame passes

Each frame the main loop runs, in order: the move and cull passes that advance
entities and drop off-screen ones, then the two player-collision passes
(`call 0xc343` graze, `call 0xd8e1` body contact, see [combat.md](combat.md)),
then the draw passes. A move/cull pass reads the front generation, advances each
surviving entity by its velocity, culls anything past the screen bounds
(`+-0x1200` x, `0xa00 / 0xff60` y), and compacts the survivors into the back
generation.

# Entity buffers: the level engine's object pools

How the in-level engine (`LEVEL_N.WAD`) stores moving objects: the player's
shots, enemies, enemy fire, pickups, and effects. Traced from `LEVEL_1.WAD` in
objdump. Underpins the damage model in [combat.md](combat.md).

Addresses are r2/objdump flat vaddr (`file = vaddr + 0x200`); `cs:[X]` is a data
offset. Each buffer is a far segment whose first word (`seg:0x0`) is the live
entity count; records follow at `seg:0x2`, fixed stride.

## Allocation

Init `0x7e04` allocates ten segments (`call 0x2804`, size in paragraphs) and
zeroes each count. They pair into **five double-buffered logical buffers**: a
front/back segment each, flipped by a generation index that cycles `0 → 2`.

| Buffer | Segments (`cs:`) | Size (para) | Generation idx | Role |
|--------|------------------|-------------|----------------|------|
| **A** | `0x29e3 / 0x29e5` | `0x64` | `cs:0xcc5` | player ordnance — **class uncertain** |
| **B** | `0x29e7 / 0x29e9` | `0x64` | `cs:0xcc5` | enemy projectiles |
| **C** | `0x29f3 / 0x29f5` | `0x64` | `cs:0xcc5` | enemies + pickups |
| **D** | `0x29eb / 0x29ed` | `0x64` | `cs:0xccd` | player projectiles |
| **E** | `0x29ef / 0x29f1` | `0x190` | `cs:0xccf` | effects / explosions / trails |

A, B and C share one generation index (`cs:0xcc5`), so they flip together each
frame; D and E have their own. To use a buffer, code loads its segment via
`mov <seg-reg>, cs:[bx + <table>]` with `bx` = the generation index.

## Buffers, proven by their spawn/update sides

- **D `0x29eb` = player projectiles.** The fire handler spawns here (`0xb4ec`
  sets `ds = D` before the fire dispatch); minigun and secondary shots and the
  smart-bomb wave all append into D. Drawn at `0xbb66`.
- **C `0x29f3` = enemies + pickups.** Its per-frame update loop `0xd4fb`
  subtracts accumulated damage from each entity's health `[si+0x18]`; on
  death / a sentinel value it routes to the pickup handlers (charge orb
  `0xd5f3`, smart-bomb `0xd698`, extra-life `0xd6c9`; sentinels like `0x8300`).
  So C holds everything the player shoots and collects. Targeted by the homing
  weapon 4 and the full-zero body-contact collision (`0xd8e1`). Drawn at `0xb577`.
- **B `0x29e7` = enemy projectiles.** Spawned by the enemy-fire routine `0xdc35`,
  which reads each enemy in C and emits bullets per that enemy's fire pattern
  (`[si+0x1b]` → table `cs:0x46b3`). Tested by the bullet-graze collision
  (`0xc343`).
- **E `0x29ef` (big) = effects.** Hit sparks (`0x34da`), projectile trails
  (`0x365a`), explosions. The collision and move routines spawn into E as `fs`;
  drawn every frame at `0xb5aa`.

The C→B link is the proof of the ship/bullet split that the damage model rests
on: enemies (C) fire bullets (B); body contact with C zeroes the bar, a graze
from B chips one level.

## Per-frame passes

Each frame the main loop runs, in order: the move/cull passes that advance
entities and drop off-screen ones, then the two player-collision passes
(`call 0xc343` graze, `call 0xd8e1` body contact — see [combat.md](combat.md)),
then the draw passes. A move/cull pass reads the front generation, advances each
surviving entity by its velocity, culls anything past the screen bounds
(`±0x1200` x, `0xa00 / 0xff60` y), and compacts the survivors into the back
generation.

## Buffer A — uncertain

A is moving **player ordnance**, but its exact class is not confirmed. Its
move/cull loop (`0xc14a`) holds the enemy buffer C in `gs` and runs a per-entity
collision (so it damages enemies), spawns a trail effect (`0x365a` into E every
other frame, `0xc0f6`), culls off-screen, and is drawn last/on top (`0xba0b`).
That makes it player shots that hit enemies and leave a trail.

What is unresolved: the plain fire output goes to **D**, and no explicit spawn
into A was found (only the in-place compaction in its move loop). So A is
plausibly the "heavy" ordnance — the trail-leaving special weapons (weapon 3/4,
missiles) split off from D's plain shots — but it could be fed by a transfer from
another buffer. Tracing which buffer the weapon-3/4 fire helpers (`0x96de` /
`0x9735`, called from the `cb5==3/4` branches of the dispatch) write to would
settle it.

# Enemy AI: the shooter levels

The AI engine shared by the four shooter levels (1, 3, 5, 7). It covers how an
entity is dispatched to its behavior function, the entity record the functions
read and write, the per-level constants, and the boss and gate machinery. The
race levels use a separate, smaller model in [race-mode.md](race-mode.md).

This is the shared model. Each level's per-function behavior is in the ported
`crates/game/src/level/lN/ai.rs`, where every function carries its original
offset as provenance. Ground truth for the shared parts is those four files plus
`crates/game/src/level/ai_common.rs` and `crates/game/src/spawns.rs`.

Two address kinds appear here. **Code** is a flat file offset (`file = vaddr +
0x200`). **Data** is a `cs:` segment offset; the per-WAD `cs` base is in the
deltas table below.

## Dispatch

Each entity is a 0x40-byte record in the C buffer. The original walks the live
buffer once per movement sub-step. For each entity it reads four words
(`lodsw` x4), leaving its source index at `entity + 8`, so every field operand
in a behavior function is written `[si + N]` for entity offset `N + 8`.

An entity's behavior is selected by two fields. While `mode == 0` (the normal
case), the behavior is `ai_table[arg * 2]`, a per-WAD pointer table. The port
mirrors this as a `match entity.arg` in each `lN::step`, gated on `mode == 0`. A
non-zero `mode` selects the scripted-path branch (6-byte `{dx, dy, dsprite}`
records with a `0xff9c` redirect sentinel); no shipped spawn row uses it, so the
port does not implement it.

Movement runs in sub-steps: each frame the engine repeats the whole pass
`cs:0xcd1` times (`max(authored, 1)`), so a fast entity moves several times per
drawn frame. The port runs this step-major rather than entity-major: it advances
every entity one sub-step, then the caller loops, culling once per sub-step. The
result is equivalent.

A seen-flag gates culling. While an entity is on screen the dispatcher sets its
`seen` byte; the cull removes an entity only once it is both off-screen and has
been seen. An entity that spawns off-screen and drifts in is never culled before
it appears. The port keeps an entity while `in_bounds || !seen`.

## Entity record

Offsets the AI reads and writes. `x` and `y` are 12.4 fixed point. The record is
0x40 bytes; the same layout serializes into a save (see [savegame.md](savegame.md)).

| Offset  | Field    | Width | Notes                                  |
| ------- | -------- | ----- | -------------------------------------- |
| `+0x00` | sprite   | word  | current sprite pointer                 |
| `+0x02` | kind     | word  | type descriptor pointer                |
| `+0x04` | x        | word  | 12.4 fixed                             |
| `+0x06` | y        | word  | 12.4 fixed                             |
| `+0x08` | hitboxes | 12 B  | three 4-byte boxes                     |
| `+0x14` | mode     | word  | 0 = behavior table, else scripted path |
| `+0x16` | arg      | word  | behavior index (mode 0)                |
| `+0x18` | health   | word  | drops on hit; death at <= 0            |
| `+0x1A` | seen     | byte  | set while on screen; gates cull        |
| `+0x1F` | anim     | byte  | animation tick                         |
| `+0x20` | tick     | word  | per-life counter                       |
| `+0x22` | phase_a  | word  | behavior scratch (reused per function) |
| `+0x24` | phase_b  | word  | behavior scratch                       |
| `+0x26` | save_y   | word  | behavior scratch                       |
| `+0x28` | save_x   | word  | behavior scratch                       |
| `+0x2A` | counter  | word  | second counter                         |

`debris` (the death template) is copied from the type descriptor at spawn, not
held as a live AI field. The scratch words from `+0x22` on are reused freely:
L1's function 4 keeps a one-shot sound flag in the low byte of `phase_a`, L5's
walker packs a path index, a form flag, and a bob value across `phase_a`,
`phase_b`, and `save_y`.

## Per-level constants

|                              | L1                  | L3                  | L5                 | L7                  |
| ---------------------------- | ------------------- | ------------------- | ------------------ | ------------------- |
| `cs` base (file = cs + base) | `0x29F0`            | `0x4710`            | `0x3F90`           | `0x51E0`            |
| AI pointer table (`cs`)      | `0xACD3`            | `0xC956`            | `0xABC5`           | `0xBDAB`            |
| Behavior functions           | 24                  | 56                  | 44                 | 50                  |
| Entity cap                   | 24                  | 48                  | 24                 | 49                  |
| Cull x (12.4, inclusive)     | -0x500..0x1200      | -0x320..0x1200      | -0x780..0x1200     | -0x3c0..0x1840      |
| Sprite descriptor            | 8-byte cycle frames | 0x1e-byte per frame | 8-byte (boss 0x1e) | 0x1e-byte per frame |

Cull y is shared: -0x3c0..0xa00. When the buffer is full the original drops the
new spawn; the port does the same. The per-function offset index for each level
lives in `lN/ai.rs` (the `match` arms) and, in full, in the RE notes.

## Boss and gate machinery

One boss runs at a time, so each boss keeps its phase state in `cs:[...]`
globals outside the entity record. The save offsets for those globals are
tabulated per level in [savegame.md](savegame.md); the live behavior is in each
`lN/ai.rs` `BossState`.

A shared **gate** counter freezes the level while a scripted set-piece plays:
while it is above zero the timer ISR holds the spawn clock, the elapsed counter,
and the parallax scroll. Each level raises it from different events (an orbiter
init, a boss reaching a phase tick, a sweeper settling) and clears it when the
gating entity dies. The port carries one `gate: u8` shared across levels.

|                    | L1       | L3       | L5       | L7       |
| ------------------ | -------- | -------- | -------- | -------- |
| Gate global (`cs`) | `0x269C` | `0x394E` | `0x2689` | `0x2ED4` |

The **plasma proximity bypass**: while the player fires plasma (`cs:0xcb5 == 3`),
the orbiter and beetle attack animations skip their vertical-proximity gate, so
the claw animates even off the player's row. L1 and L3 reproduce this through
`ctx.firing_plasma`. L5's boss instead fires four times as fast under plasma;
L7 has no aiming enemy and no plasma gate.

Behavior functions read their motion curves straight from the WAD image: a
12.4-delta wave table at a file offset, fetched with `word(wad, at)` and shifted
`<< 4`. The port's fetch returns 0 past the end of the image as a guard; the
original reads whatever trailing bytes follow.

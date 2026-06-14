# Savegame: the `.psg` format

A `protosgN.psg` file is a full mid-level snapshot the level writes from its
in-game menu (the writer at `LEVEL_2.WAD` file `0xb8da`, the loader at
`0xb9d3`). The port reads and writes the original's format unchanged, so a save
made in DOSBox loads in the port and the reverse. Ground truth is
`crates/game/src/savegame.rs`, validated field-by-field against real DOSBox
saves of all seven levels (`crates/game/tests/fixtures/`).

Addresses here are `cs:` segment offsets, the level's live game variables. The
block begins at `cs:0xcb4`, so a variable's byte position inside the file is its
`cs:` offset minus `0xcb4`. The object buffers follow the block. Everything is
little-endian, with no header and no padding.

## File layout

A file is the variable block followed by both parity copies of each live-object
buffer, in this order in every WAD:

| Region | Contents                                               | Copies | Copy length |
| ------ | ------------------------------------------------------ | ------ | ----------- |
| Block  | The level's variable block; byte 0 is the level number | 1      | per WAD     |
| A      | Player shots                                           | 2      | `0x640`     |
| B      | Enemy shots                                            | 2      | `0x640`     |
| C      | Entities                                               | 2      | per WAD     |
| D      | Fire staging                                           | 2      | `0x640`     |
| E      | Effects                                                | 2      | `0x1900`    |

The block is one congruent structure in all seven WADs, relinked at per-WAD
addresses. The spawn table runs from its base to the cursor word (whose baked
initial value is the base), then the scroll list and its accumulators, then the
ship cluster, then the score cluster, with each level's own AI state woven
around them. The three race WADs (LEVEL_2/4/6) share every internal delta but
not their anchors: each race's spawn table has its own length, which shifts the
cursor and everything after it.

## Per-WAD geometry

| WAD     | Level byte | Block length | C-buffer length | File size |
| ------- | ---------: | -----------: | --------------: | --------: |
| LEVEL_1 |          1 |     `0x1A09` |    `0x640` (24) |    32 265 |
| LEVEL_2 |          2 |     `0x1BE1` |    `0xC80` (49) |    35 937 |
| LEVEL_3 |          3 |     `0x2CAF` |    `0x640` (24) |    37 039 |
| LEVEL_4 |          4 |     `0x1C59` |    `0xC80` (49) |    36 057 |
| LEVEL_5 |          5 |     `0x19EA` |    `0xC80` (49) |    35 434 |
| LEVEL_6 |          6 |     `0x2159` |    `0xC80` (49) |    37 337 |
| LEVEL_7 |          7 |     `0x2235` |    `0x640` (24) |    34 357 |

The C-buffer count is `(length - 2) / 0x40`: 24 entities for L1/L3/L7, 49 for L5
and the races. The decode rejects any file whose length does not match its level
byte.

## Block anchors

These `cs:` offsets differ per WAD; every other block field sits at a fixed
delta from them.

| WAD     | Table base | Cursor   | Score    | Win     | Parity  | Stage   | Reload  | Gate     |
| ------- | ---------- | -------- | -------- | ------- | ------- | ------- | ------- | -------- |
| LEVEL_1 | `0xCEC`    | `0x25EC` | `0x268F` | `0xCC3` | `0xCC5` | `0xCDE` | `0xCC8` | `0x269C` |
| LEVEL_2 | `0xCE6`    | `0x27FE` | `0x2873` | `0xCC3` | `0xCC5` | `0xCDE` | `0xCC8` |          |
| LEVEL_3 | `0xD06`    | `0x38C6` | `0x3941` | `0xCC3` | `0xCC5` | `0xCFE` |         | `0x394E` |
| LEVEL_4 | `0xCE6`    | `0x2876` | `0x28EB` | `0xCC3` | `0xCC5` | `0xCDE` | `0xCC8` |          |
| LEVEL_5 | `0xCF1`    | `0x25F1` | `0x267C` | `0xCC3` | `0xCC5` | `0xCDE` | `0xCC8` | `0x2689` |
| LEVEL_6 | `0xCE6`    | `0x2D76` | `0x2DEB` | `0xCC3` | `0xCC5` | `0xCDE` | `0xCC8` |          |
| LEVEL_7 | `0xD02`    | `0x2C52` | `0x2EC7` | `0xCDF` | `0xCE1` | `0xCFA` | `0xCE4` | `0x2ED4` |

LEVEL_7 carries extra prefix fields that shift its whole flag/parity cluster up
by the boss globals it stores in front of it (see the boss table below), which
is why its win and parity anchors sit higher than the others'. L3 inserts its
own flags inside the fire cluster, so its reload byte's position is not pinned
and the codec leaves it unset.

### Fields derived from the anchors

- Scroll list pointer and count: cursor `+2`.
- Scroll accumulators: cursor `+6`, one dword each, followed by the per-tick
  rate constants (one dword each).
- Ship cluster (`ramp`): after the accumulators and their constants
  (`accums + count * 8`). It holds the fly-in ramp counter, ship x/y, the two
  7-word trail history rings, the roll offset (stored `roll * 0x12`), the
  weapon-upgrade-drop countdown, lives, invincibility, and, on the races, the
  contact-grace word.
- Camera pair (`camera * 0x50`, then the camera row): `ramp + 0x2A`, plus 2 on
  the races for their contact-grace word. The races store their speed in the
  camera row.
- Score cluster: a six-digit readout scratch and two caches precede the score
  dword; the extra-life decade threshold follows it.

## Object buffers

Each buffer is a count word at `+0` followed by fixed-size records. The record
field layouts are the in-memory entity layouts, documented in
[entity-buffers.md](entity-buffers.md): shots and effects are 16 bytes, entities
are `0x40`. Capacities are 99 shots (A/B/D), 24 or 49 entities (C, per WAD), and
399 effects (E).

Each buffer is double-buffered. The parity words select which copy is current:
the word at the parity anchor (divided by 2) for A/B/C, and the word at parity
`+0xa` for E. The port writes both copies identically and names copy 0 as
current.

Three of the five buffers carry no live state through a save:

- **A (player shots)** is deliberately dropped. The original preserves it, but
  the port models a player shot as a kind plus an octant rather than the raw
  sprite pointer, and the pointer resolves per-level from the WAD. In-flight
  player shots live well under a second and self-correct on the first unfrozen
  tick, so reconstructing them is not worth coupling the WAD into the codec.
- **D (fire staging)** is merged into A every frame and saved empty.
- Enemy-shot **size and damage** (`+0xa`, `+0xe`) are left zero: the spawn
  helper copies size from the sprite descriptor and the hit path ignores
  damage, both re-derived at collision time, so storing them would need the WAD.

See [deviations.md](deviations.md) for the full list.

## Boss state

Each boss keeps its phase state in `cs:[...]` globals inside the block rather
than in an entity record, so the codec carries them separately. A save taken
mid-boss-fight resumes the boss in place rather than restarting its pattern. The
races have no boss and carry none. Ground truth is each level's `BossState` in
`crates/game/src/level/lN/ai.rs`.

### LEVEL_1 (`gate cs:0x269c`)

| `cs:`    | Field             | Width |
| -------- | ----------------- | ----- |
| `0xCE8`  | `form2`           | byte  |
| `0xCE9`  | `dying`           | byte  |
| `0x269D` | `anchor_x`        | word  |
| `0x269F` | `anchor_y`        | word  |
| `0x26A1` | `saved_a`         | word  |
| `0x26A3` | `saved_b`         | word  |
| `0x26A5` | `fire_timer`      | word  |
| `0x26A7` | `explosion_timer` | word  |

### LEVEL_3 (`gate cs:0x394e`)

| `cs:`   | Field             | Width |
| ------- | ----------------- | ----- |
| `0xCD7` | `tick`            | word  |
| `0xCD9` | `bob_phase`       | word  |
| `0xCDB` | `frame_index`     | word  |
| `0xCDD` | `creep_x`         | word  |
| `0xCDF` | `home_x`          | word  |
| `0xCE1` | `lunge_end_x`     | word  |
| `0xCE3` | `sine_index`      | word  |
| `0xCE5` | `hover_count`     | word  |
| `0xCE7` | `pattern_count`   | word  |
| `0xCE9` | `divider`         | byte  |
| `0xCEF` | `fire_timer`      | word  |
| `0xCF1` | `explosion_timer` | word  |
| `0xCF3` | `explosion_dx`    | word  |
| `0xCF5` | `explosion_dy`    | word  |

### LEVEL_5 (`gate cs:0x2689`)

| `cs:`   | Field        | Width |
| ------- | ------------ | ----- |
| `0xCE4` | `tick`       | word  |
| `0xCE6` | `phase`      | byte  |
| `0xCE7` | `half_rate`  | byte  |
| `0xCE8` | `bounce`     | byte  |
| `0xCE9` | `fire_ticks` | word  |

The L5 explosion-burst timer lives at `cs:0xa734`, outside the saved block, and
restores at its idle default.

### LEVEL_7 (`gate cs:0x2ed4`)

The composite boss is five parts driven from one shared state. Its globals
occupy `cs:0xcc3..0xcdd`, the region other WADs use for the win and parity
flags, which is why LEVEL_7's flag cluster is shifted up.

| `cs:`   | Field            | Width |
| ------- | ---------------- | ----- |
| `0xCC3` | `anchor_x`       | word  |
| `0xCC5` | `anchor_y`       | word  |
| `0xCC7` | `wobble_x`       | word  |
| `0xCC9` | `wobble_y`       | word  |
| `0xCCB` | `master_tick`    | word  |
| `0xCCD` | `pattern_clock`  | word  |
| `0xCCF` | `anim_gate`      | byte  |
| `0xCD0` | `spiral_phase`   | word  |
| `0xCD2` | `spiral_divider` | byte  |
| `0xCD3` | `shared_health`  | word  |
| `0xCD5` | `smoke_dx`       | word  |
| `0xCD7` | `smoke_dy`       | word  |
| `0xCD9` | `smoke_delay`    | word  |
| `0xCDB` | `wobble_phase_x` | word  |
| `0xCDD` | `wobble_phase_y` | word  |

## Resume through GET READY

The freeze and dying flags and the engine PRNG live outside the saved ranges, so
a loaded game always resumes through GET READY on a fresh clock seed, exactly as
the original does. This is why the transient fire state (cooldown, muzzle flash,
the orb deploy machine) is not carried: it is unobservable across a GET READY,
and re-resolves on the first unfrozen tick in both engines.

Two block fields are written to their baked image values rather than zero,
because the original engine never re-initializes them on load and a written zero
would corrupt the rest of a session:

- The fire **reload** byte is written as 6. The consumer is an
  `incb cooldown; cmp reload; jb` pair, so a zero reload fires every tick until
  the next weapon resolve.
- The pod/orb **stage** byte is written as 1 (the hold stage). The orb
  consumers compare against 1..4 and nothing else writes it, so a zero stage
  kills the orb machine for the rest of a session.
- On the races, the HUD alternation flag (score `- 0x11`) is written as 1. The
  original toggles it with `notb`, so a written zero sticks the alternation
  off-phase forever.

## START.EXE handoff

START.EXE's LOAD path hands the chosen slot to the level the same way NEW GAME
hands over a fresh state: the carried score, lives, bombs, and weapon levels
travel as the `Handoff` view of the decoded `GameState` (`SaveGame::handoff`).
The front-end side of the LOAD menu is in [start-exe.md](start-exe.md).

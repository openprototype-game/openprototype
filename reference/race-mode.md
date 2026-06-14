# Race mode

LEVEL_2, LEVEL_4, and LEVEL_6 are the race levels. They share one code base and
swap obstacle layouts. They are set in space: the scrolling band behind the ship
is the RACEB2 nebula (an SP background), not a road. Ground truth is
`crates/game/src/level/race/ai.rs`, `crates/game/src/level/spawn.rs`, and the
race entries in `crates/game/src/levels.rs`.

Addresses are `cs:` segment offsets (`file = cs + 0x29F0`) unless marked as a
file offset. The data layout holds across all three WADs; every code anchor is
relinked per WAD, so any offset outside the spawn table must be re-derived per
level (see the L4/L6 table).

## The model

A race is the shooter engine with its weapons and pickups intact and its combat
defanged:

- **Speed is cosmetic.** It drives the road renderer and the vertical camera and
  nothing else: no effect on spawning, scoring, or collision.
- **Obstacles are indestructible.** Each spawns with a symbolic 32000
  (`0x7d00`) health and is never killed by a shot or a ram (`ram_survivors` is
  the catch-all `(0, 0xffff)`).
- **Contact is survivable.** Hitting an obstacle drains the active weapon and
  arms 120 ticks (`0x78`) of contact grace rather than killing the ship.
- **A crash restarts the course.** Running out of weapon to absorb a hit (or any
  fatal contact) restores the spawn table, resets the cursor to record 0, resets
  the scroll, and wipes live entities (`course_restart`). Speed, score, weapons,
  bombs, effects, and player shots carry across the restart.
- **No enemy fire.** There is no fire pass; the enemy-shot buffer stays empty.
- **Entry costs a life.** Each entry decrements lives; at zero lives the level
  exits with game-over status 5. The death sequence runs 92 ticks (23 frames).

## Speed and camera

Speed is a level 0..0x20 (`cs:0x2852`) with a derived pixel offset
`cs:0x2850 = speed * 0x50` (one 80-byte buffer row per step). Holding down while
the ship sits at y >= 60 builds speed by one per tick to a cap of 0x20; holding
up at y <= 50 sheds it to a floor of 0. The VRAM window shows 128 rows starting
at `cs:0x2850 + 0x50 + 4`, so building speed slides the visible window down by up
to 32 rows. The camera floor (`camera_min`) is 0. The nebula scrolls at a
constant per-strip rate with no shear: every row shows the same wrapped column.

## Obstacle behavior

Race obstacles use a 6-entry behavior table (file `0xa476` on L2). Every function
drifts the entity left by 0x50 (5 px) per sub-step except the stationary finish.

| arg | Behavior                                                       |
| --: | -------------------------------------------------------------- |
|   0 | drift; weapon-upgrade pickup, animates every 4 ticks           |
|   1 | drift; smart-bomb pickup                                       |
|   2 | drift; invincibility pickup                                    |
|   3 | drift; extra-life pickup (unused by L2's table)                |
|   4 | drift, no animation: the plain obstacle                        |
|   5 | stationary; sets the level-end flag `cs:0xcc3 = 1`: the finish |

Pickups cycle frames on every 4th step from `rest + 0x1e` through the type's last
frame and back. The port's per-arg last-frame offsets are `[0x5e, 0x5e, 0x6e,
0x46]`.

## Spawn layout

Races bake their layout into the WAD instead of generating it. The schedule is a
static table at file `0x1696` (`SpawnSource::StaticTable`), shared by all three
WADs, of `{delay, sprite, health, spawn_row}` words: the same record shape the
shooter consumer reads. A record with sprite word 0 ends the run; every WAD's
terminator is `{20, 0, 0, 0}`. The live entity cap is 49.

`spawn_row` indexes a per-WAD position table (L2 at file `0x37b2`, 210 rows):

| Rows   | Layout                                                             |
| ------ | ------------------------------------------------------------------ |
| 0..8   | pickups at x 288, y 30/60/90, args 1/2/3                           |
| 9..208 | obstacle grid: x = 288 + (row-9)/40, y = ((row-9) % 40) * 4, arg 4 |
| 209    | the finish entity at x 290, y 0, arg 5                             |

## Finishing

The last live record (`{400, sprite, 0x14, 209}`) spawns the finish entity. On
its first sub-step it sets `cs:0xcc3 = 1`, the level-end flag the port carries as
`level_end`. A win flyout then runs 460 frames; over the last 300 it re-pins the
ship's fly-in ramp to send it off the right edge at 2 px/tick, then exits. The
only lose path is running lives to zero, which exits with status 5; there is no
time limit. The race has no boss and no gate, encoded in the port as the
sentinels `gate_release: (1, 0)` and `level_end_sprite: 0xffff`.

## L4 and L6

The three races share the table offset and all data shapes; counts and relinked
code anchors differ.

|                  | L2       | L4       | L6       |
| ---------------- | -------- | -------- | -------- |
| WAD              | LEVEL_2  | LEVEL_4  | LEVEL_6  |
| Records          | 67       | 82       | 242      |
| Finish sprite    | `0x3E1C` | `0x3E94` | `0x4394` |
| Position table   | `0x37B2` | `0x382A` | `0x3D2A` |
| Catalog offset   | `0xBF5A` | `0xBFD6` | `0xC4D6` |
| Entity cell base | 695      | 705      | 833      |
| AI table (file)  | `0xA476` | `0xA4EE` | `0xA9EE` |
| Ship-rect table  | `0x4237` | `0x42AF` | `0x47AF` |

## Completion and a latent edge

A race completes the same way a shooter does: the finish entity raises the
level-end flag (`cs:0xcc3`), which the port carries as `level_end` and the level
scene turns into the win flyout and the next-level handoff. The original's
separate `cs:0xcc4` "completed" analog (checked at file `0x6ff4`) is never
written, so it is dead; the port needs nothing more to tell a finished race from
a game-over.

One latent edge stays faithful rather than guarded: the finish entity carries
health 20, so a player shot reaching past its spawn x the frame after it appears
could in principle kill it before its AI sets the level-end flag, leaving the
race unwinnable. The hit-test geometry makes this hard (shot hit tests stop at
x <= 0x120, the gate's first box starts near 290) but it is not proven
impossible. The port keeps the original's health value as-is.

# Sound and music

The in-level audio: a three-channel SoundBlaster sample mixer for effects and a
CD audio track for music. Ground truth is `crates/game/src/sfx.rs` and the
`SfxData` entries in `crates/game/src/levels.rs`.

## Channels

Effects mix on three channels, assigned by category:

| Channel | Use                                  |
| ------: | ------------------------------------ |
|       0 | explosions and impacts               |
|       1 | player weapons                       |
|       2 | pickups, enemy sounds, weapon switch |

A trigger overwrites whatever its channel is playing, with no fade and no
priority. The one exception is the chaingun hit, which does not interrupt a busy
channel (`skip_if_busy`). The original mixes additively (it wraps on overflow);
the port saturates instead, a deliberate deviation in
[deviations.md](deviations.md).

## Sample slots

Each WAD bakes a table of 16-byte sample filenames, loaded whole at level start;
effects are addressed by slot index. The slot meanings are positional and the
same across levels except slot 8 (the per-level enemy sound) and L5's extra slot
16.

| Slot | Sample                       |
| ---: | ---------------------------- |
|    0 | chaingun                     |
|    1 | chaingun hit                 |
|    2 | weapon switch (changewe)     |
|    3 | pod death                    |
|    4 | explosion                    |
|    5 | asteroid death               |
|    6 | pebble (weapon-upgrade drop) |
|    7 | pickup                       |
|    8 | enemy sound (per level)      |
|    9 | missile impact               |
|   10 | missile                      |
|   11 | plasmagu (burning fire)      |
|   12 | enemy shot                   |
|   13 | drain                        |
|   14 | burning2 (plasma hum loop)   |
|   15 | multisho (multishot fire)    |
|   16 | L5 only: extra sample        |

The slot-8 enemy sample is `gegrocke` on L1 and the races, `lgegshot` on L3,
`kanone` on L5, `scheren` on L7.

## Per-level data

Each level's `SfxData` points at its WAD name table and its authored length
table, and names its CD track.

| Level | Name table | CD track | Length table       |
| ----- | ---------: | -------: | ------------------ |
| L1    |   `0x5229` |        3 | `L1_SFX_LENGTHS`   |
| L2    |   `0x33C3` |        7 | `RACE_SFX_LENGTHS` |
| L3    |   `0x81F0` |        5 | `L3_SFX_LENGTHS`   |
| L4    |   `0x343B` |        7 | `RACE_SFX_LENGTHS` |
| L5    |   `0x67AD` |        4 | `L5_SFX_LENGTHS`   |
| L6    |   `0x393B` |        7 | `RACE_SFX_LENGTHS` |
| L7    |   `0x8246` |        6 | `L7_SFX_LENGTHS`   |

## Authored lengths

Each sample is cut to an authored length at load (the files run a couple hundred
bytes longer than the trigger uses). The length table's entry count also sets the
slot count. Values are byte counts.

| Slot |       L1 |     Race |       L3 |       L5 |       L7 |
| ---: | -------: | -------: | -------: | -------: | -------: |
|    0 | `0x1920` | `0x1920` | `0x1920` | `0x1920` | `0x1920` |
|    1 |  `0x17E` |  `0x17E` |  `0x17E` |  `0x17E` |  `0x17E` |
|    2 | `0x2BFC` | `0x2BFC` | `0x2BFC` | `0x2BFC` | `0x2BFC` |
|    3 | `0x1A92` | `0x1A92` | `0x1A92` | `0x1A92` | `0x1A92` |
|    4 | `0x3E74` | `0x3E74` | `0x3E74` | `0x3E74` | `0x3E74` |
|    5 | `0x1840` | `0x1840` | `0x1840` | `0x1840` | `0x1840` |
|    6 | `0x22AE` | `0x22AE` | `0x22AE` | `0x22AE` | `0x22AE` |
|    7 |  `0xC50` |  `0xC50` |  `0xC50` |  `0xC50` |  `0xC50` |
|    8 | `0x2D62` | `0x2D62` |  `0xA78` | `0x1E96` | `0x207C` |
|    9 | `0x11E4` | `0x11E4` | `0x11E4` | `0x11E4` | `0x11E4` |
|   10 | `0x1E68` | `0x1E68` | `0x1E68` | `0x1E68` | `0x1E68` |
|   11 | `0x1912` | `0x1912` | `0x1912` | `0x1912` | `0x1912` |
|   12 |  `0x6F4` |  `0x6F4` |  `0x6F4` |  `0x6F4` |  `0x6F4` |
|   13 |  `0x606` |  `0x62C` |  `0x62C` |  `0x62C` |  `0x62C` |
|   14 | `0x1482` | `0x1482` | `0x1482` | `0x1482` | `0x1482` |
|   15 |  `0x604` |  `0x606` |  `0x606` |  `0x606` |  `0x606` |
|   16 |        — |        — |        — |  `0xA78` |        — |

L1 trims slot 15 (multisho) two bytes shorter and slot 13 (drain) to `0x606`;
every other level uses `0x606` and `0x62C` there.

## Triggers

Game events map to slots and channels as follows. Only the plasma hum loops.

| Event                        | Slot                                  | Channel |
| ---------------------------- | ------------------------------------- | ------: |
| Chaingun fired               | chaingun (0)                          |       1 |
| Multishot fired              | multisho (15)                         |       1 |
| Burning fired                | plasmagu (11)                         |       1 |
| Plasma fired                 | burning2 (14), looped                 |       1 |
| Plasma ball launched         | ends the ch1 loop                     |       1 |
| Weapon resolved to a new one | changewe (2)                          |       2 |
| Weapon bar hit zero          | changewe (2)                          |       2 |
| Enemy died (asteroid)        | asteroid death (5)                    |       0 |
| Enemy died (pod)             | pod death (3)                         |       0 |
| Enemy died (other)           | explosion (4)                         |       0 |
| Ship died                    | explosion (4)                         |       0 |
| Boss explosion               | asteroid death (5) then explosion (4) |       0 |
| Pickup collected             | pickup (7)                            |       2 |
| Weapon-upgrade dropped       | pebble (6)                            |       2 |
| Extra life                   | pickup (7)                            |       2 |
| Weapon drained one level     | drain (13)                            |       2 |
| Enemy fired                  | enemy shot (12)                       |       2 |
| Chaingun round hit           | chaingun hit (1)                      |       0 |
| Missile hit                  | missile impact (9)                    |       0 |
| AI-triggered enemy sound     | caller's slot                         |       2 |

The weapon-switch sound fires when the firing weapon **resolves** to a different
one (the per-tick resolve), not on the switch key. Orb bolts and multishot-family
impacts are silent.

### Plasma hum

Plasma is the only looping effect. The first plasma firing tick plays burning2 on
channel 1 with the loop flag set (`cs:0x8414` on L1); held ticks while the flag
is set are no-ops. Launching the plasma ball clears the flag and ends the loop;
the launch itself is silent and the current pass plays out.

## Music

Music is a CD audio track (`LevelData.music_track`), started by the level. The
engine loops the track by timer: the track's table-of-contents length in 60 Hz
ticks (`floor(TOC_frames / 75) * 60`) counts down in the timer ISR, gated on the
music-enabled flag, and restarts the track when it underflows. The countdown
keeps running while the game is frozen.

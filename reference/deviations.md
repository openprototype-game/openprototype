# Deviations from the original

Where OpenPrototype deliberately differs from the 1995 DOS game, with the
reasoning. The standing rule: the port reproduces the original's *design*,
including odd-but-intentional behavior, but does not reproduce accidental
engine bugs or crashes. Everything here is a conscious choice, not an
unfinished port; faithfulness gaps that are merely unimplemented live on the
roadmap instead.

## Original bugs, deliberately not reproduced

- **Enemy shots arming the plasma-hum loop flag.** The original's
  enemy-shot trigger writes the plasma hum's loop flag (L1 `0xae51` →
  `cs:0x8414`), so a plasma hold after an enemy volley plays no hum until a
  launch clears the flag, and an enemy shot during the post-launch playout
  re-arms the dying hum. A shared-variable slip, not a designed effect; the
  port keeps the hum independent.
- **Music auto-start under a long GET READY.** The loop countdown is baked
  at 1000 ticks and the ISR decrements it while frozen, so an undismissed
  opening GET READY auto-starts the track after ~16.7 s and the dismissal
  then restarts it. A race between the countdown and the pending-start flag;
  the port starts the track at the first unfreeze only.
- **Additive mix overflow.** The original sums its three sample channels into
  the DMA buffer and wraps on overflow, so loud simultaneous effects clip into
  noise. The port saturates instead, keeping the same mix at normal levels
  without the wrap artifact (see [sfx.md](sfx.md)).
- **Sample tail past the trigger length.** The original frees a mixer channel
  only at a 250-byte DMA block boundary, so it plays up to ~249 bytes of file
  data past a sample's authored length. The port cuts each sample at its
  authored length (see [sfx.md](sfx.md)), dropping the block-boundary tail.

## Crashes the port degrades instead of reproducing

- **Point-blank aimed shots.** The enemies' aim-at-player helpers divide by
  the player distance with no guard (L1 `0xde16`, L3 `0x117f9`, L5
  `0xf2d8`); a zero scale (the shielded ship sitting inside the shooter)
  faults the original to DOS. The port skips the shot, the same shape on
  every level (the per-level guards used to differ).
- **Pool overflows.** The original raises fatal error exits at 0x5f live
  shots, 0x18f live effects, and the entity cap; the port drops the overflow
  and keeps running. The entity cap matches the original's *sustained* max
  of cap − 1 (its fatal fires when the cap-th survivor is written).
- **Level error / load failures.** Exit status 3 paths and an unreadable
  save: the original exits to DOS; the port panics or, for a bad save slot,
  falls back to the menu.

## Intentional design choices

- **In-game QUIT exits the app** (matching the original's exit-to-DOS), not
  back to the menu.
- **The ERIK cheat is a typed sequence** (`e-r-i-k`, each letter within a
  second, while running) rather than the original's simultaneous 4-key hold.
  The original chord ghosts on many modern keyboard matrices; it only ever
  had to work on the NEO team's own boards (ERIK tops the shipped highscore
  list). The grant is the original's: 0x7d00 invincibility ticks and all
  four weapon bars full.
- **GRAPHICS... and JOYSTICK... draw disabled** (the dim-text treatment, and
  the cursor skips them) until their submenus land: the detail-level system
  and gamepad support, both on the roadmap. The original's items work; a
  dimmed item reads more honestly than a working-looking inert one.

## Faithful but mechanically different

These match the original's observable result by a different mechanism, so
they are not really deviations, only noted to forestall "the port does X
differently" reports.

- **Zoom pacing.** The original's 0x2870 scaler is CPU-bound and unpaced;
  the port paces it (1 tick/step for the ending, 125 ms/step for the
  highscore fly-in) to the same visual ramp.
- **Edge-triggered input.** The port reacts to key press/release events
  where the original polls held-state flags each tick; this already covers
  the original's Esc-release debounce by construction.
- **Tick batching.** The original advances entity movement in sub-steps within
  a frame but culls, collides, and draws once per frame; the port advances the
  whole simulation (spawn pull, move, cull, collide, compose) in whole logic
  ticks under a catch-up fixed timestep. The two are identical at the target
  tick rate; only under host lag (several ticks per rendered frame) does the
  cull cadence differ.
- **Decoder OOB guards.** The format decoders carry defensive bounds the
  original lacks; unreachable on shipped data, golden-verified.
- **FLI skip-on-undecodable.** The port skips a corrupt chunk; the original
  bails the whole animation cleanly. Both reach the same end state on
  shipped data.
- **Save slots** live in the OS data directory rather than beside the
  executable; the `.psg` byte format is preserved exactly.
- **In-flight player shots are not saved.** The original preserves the
  player-shot buffer across a save; the port writes it empty (`savegame.rs`).
  The port models a shot as a kind plus an octant, not the raw sprite pointer,
  and a shot lives well under a second; a load resumes through GET READY, so the
  buffer self-corrects on the first unfrozen tick (see [savegame.md](savegame.md)).

## Open / latent assumptions

- **Plasma ball damage after a mid-retract bomb.** The launched ball never
  gets a damage byte written; it inherits the staging slot (normally 30). A
  smart bomb fired during the orb retract *might* leave it at 0 in the
  original, unverifiable by eye (the bomb kills every viable target), so
  the port's fixed 30 stands as the assumption.
- **Stale spawn fields.** The original's entity builders leave tail fields
  (phase/save words) holding the previous slot occupant's bytes; the port
  zero-inits. Visible only as a one-cycle phase offset on L5 path shooters
  and uniform orb bobbing. Documented rather than reproduced (it would need
  the original's fixed-slot memory model).
- **L1 orb-pickup fire pattern.** A live but inert fire pass in LEVEL_1.WAD
  only: converted pickups arm pattern 0 and would fire aimed shots, but the
  shots render at a 16×-scaled position and connect with nothing
  (DOSBox-confirmed). Not ported.

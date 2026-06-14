# FLI: Autodesk Animator animations

Status: verified by decoding every file and rendering every frame.

The game's animations are the FLI variant of the Autodesk FLIC format. Twelve are
shipped: the front-end movies (INTRO, FLY, HIGHSCOR, CREDZ, GO2), the level
backdrops (CANYON, LAVA), and the five between-level cutscenes (SPACE1, SPACE2,
SPACE3, WALDENDE, TEND).

## Layout

- File header: 128 bytes. Magic `0xAF11` at offset 4, frame count at 6, width at
  8, height at 10, `speed` at 16 (frame delay in 1/70-second jiffies). All files
  are 320x200, 8 bits per pixel.
- Frame header: 16 bytes. Frame size (`u32`), magic `0xF1FA`, chunk count
  (`u16`), 8 reserved.
- Chunk header: 6 bytes. Chunk size (`u32`, includes the header), type (`u16`).

Frame 0 is a full keyframe; every later frame is a delta against the previous
one. The decoder keeps a persistent canvas and applies each frame's chunks on
top. Clearing the canvas between frames corrupts every delta frame.

## Chunk types

Four appear across the shipped files:

- **COLOR64** (11): `u16` packet count, then `(skip, count)` packets. `skip`
  advances the color index, `count` colors follow as 6-bit RGB triples
  (`count == 0` means 256). Same 6-bit-to-8-bit expansion as `.PAL`.
- **BRUN** (15): the keyframe. Per line, a leading packet count (ignored, fill to
  width) then packets. Signed count byte: positive is a run of the next byte,
  negative is a literal copy.
- **LC** (12): the delta. `u16 first_line`, `u16 num_lines`, then per line a
  packet count and `(skip, count)` packets. The sign is the opposite of BRUN:
  positive is a literal copy, negative is a run of the next byte. Lines outside
  the range and skipped pixels keep their previous-frame values.
- **COPY** (16): an uncompressed full frame, 64000 bytes row-major. Only
  LAVA.FLI has them: 28 chunks at frames 20-23, 67, and 71-93 (the last 23 are
  COPY-only frames). Quirk: every one of those chunks declares its size as 64004
  (a 63998-byte body), but the handler (START.EXE file `0x31ca`, `rep movsl` of
  0x3e80 dwords) copies 64000 bytes regardless, so the frame's last 2 pixels come
  from past the chunk: the low word of the next frame header's size field. A
  pixel-faithful decoder reads those 2 bytes from the file rather than clamping
  to the declared body.

The sign convention flips between BRUN and LC.

## Playback

START.EXE ignores the header `speed` field. Its players wait a caller-set tick
count (`cs:[0x3022]`, 1/70 s units) after every frame, including the last. The
front-end movies run at intro 3 ticks/frame, fly 2, highscor 4, credz 8. The
intro player (`0x31fd`) plays exactly the header frame count; the credits player
(`0x3293`) plays one frame fewer and composites text over each frame.

The chain loop plays a between-level movie before launching every level past the
first (file `0x3d0a`), from a drive-patched table at vaddr `0x36f4` indexed by
the level about to launch. A single key-down skips it (the int9 counter
`cs:[0x2dea]` must read exactly 1, so two keys held at once do not skip). The
movie after the finished level, and its frame delay in ticks:

| Finished level | Movie        | Ticks/frame |
| -------------- | ------------ | ----------: |
| L1             | CANYON.FLI   |           1 |
| L2             | SPACE1.FLI   |           4 |
| L3             | WALDENDE.FLI |           4 |
| L4             | SPACE2.FLI   |           4 |
| L5             | TEND.FLI     |           1 |
| L6             | SPACE3.FLI   |           4 |
| L7             | LAVA.FLI     |           1 |

LAVA.FLI (after L7) leads into the ending sequence rather than another level.

The port decodes and plays these in `crates/game/src/flic_player.rs`, driven by
`scene/intro.rs` (the front-end movies), `scene/transition.rs` (the between-level
table above), and `scene/ending.rs` (the post-LAVA ending).

## Notes

- No `COLOR256` (4), `SS2` (7), or `BLACK` (13) chunks are present in any file,
  so the decoder handles the four above and ignores any other type. (The original
  aborts the whole animation on an unknown type, unreachable on shipped data.)
- The palette comes from the COLOR64 chunk in frame 0; it is not a separate file.

# FLI: Autodesk Animator animations

Status: verified by decoding all 7 files and rendering every frame.

The animations (intro, canyon, lava, credits, highscores, fly, go2) are the
FLI variant of the FLIC format.

## Layout

- File header: 128 bytes. Magic `0xAF11` at offset 4, frame count at 6, width at
  8, height at 10, `speed` at 16 (frame delay in 1/70-second jiffies). All files
  are 320x200, 8 bits per pixel.
- Frame header: 16 bytes. Frame size (`u32`), magic `0xF1FA`, chunk count
  (`u16`), 8 reserved.
- Chunk header: 6 bytes. Chunk size (`u32`, includes the header), type (`u16`).

Frame 0 is a full keyframe; every later frame is a delta against the previous
one. The decoder keeps a **persistent canvas** and applies each frame's chunks on
top. Clearing the canvas between frames corrupts every delta frame (the symptom a
prior attempt hit).

## Chunk types

Only three appear across every file:

- **COLOR64** (11): `u16` packet count, then `(skip, count)` packets. `skip`
  advances the color index, `count` colors follow as 6-bit RGB triples
  (`count == 0` means 256). Same 6-bit-to-8-bit expansion as `.PAL`.
- **BRUN** (15): the keyframe. Per line, a leading packet count (ignored; fill to
  width) then packets. Signed count byte: **positive → run** of the next byte,
  **negative → literal** copy.
- **LC** (12): the delta. `u16 first_line`, `u16 num_lines`, then per line a
  packet count and `(skip, count)` packets. Sign is the **opposite** of BRUN:
  **positive → literal** copy, **negative → run** of the next byte. Lines outside
  the range and skipped pixels keep their previous-frame values.

The flipped sign convention between BRUN and LC is the easy bug to write.

## Playback

START.EXE ignores the header `speed` field. Its players wait a caller-set tick
count (`cs:[0x3022]`, 1/70 s units) after every frame, including the last:
intro.fli plays at 3 ticks/frame, fly.fli at 2, highscor.fli at 4, credz.fli
at 8. The intro player (`0x31fd`) plays exactly the header frame count; the
credits player (`0x3293`) plays one frame fewer and composites text over each
frame. Details in `reference/start-exe.md`.

## Notes

- No `COLOR256` (4), `SS2` (7), `BLACK` (13), or `COPY` (16) chunks are present,
  so the decoder handles the three above and ignores any other type.
- The palette comes from the COLOR64 chunk in frame 0; it is not a separate file.

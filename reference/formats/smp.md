# SMP: sound effects

Status: verified by ear and against the engine's DSP programming.

The `.SMP` files are the game's sound effects (gunfire, explosions, pickups).
The CD-audio music is not here; it played as red-book tracks on the original CD.

## Format

- Raw 8-bit PCM, mono, no header: the whole file is samples.
- **Signed** on disk (silence = 0). The Sound Blaster DSP plays 8-bit DMA as
  unsigned, so the engine adds 128 when copying samples into the DMA buffer. To
  play one (WAV, or the DAC), convert signed → unsigned the same way: `byte +
  128` (equivalently `byte ^ 0x80`). Evidence: every file trails off into `0x00`,
  which is silence only under the signed reading.
- **11111 Hz**. The level engine computes the SB time constant from a requested
  11000 Hz (mixer init, LEVEL_1 file `0x7a06`/`0x7802`): `256 - 1000000/11000` =
  166 (`0xA6`), and the DSP then plays `1000000 / 90` ~= 11111 Hz. 11025 can't be
  expressed as a
  time constant. Found in `LEVEL_1.WAD` at the single-cycle DMA playback routine
  (`0x40` time constant, `0xD1` speaker on, `0x14` 8-bit DMA).

## Notes

- The decoder (`smp::decode`) normalises to unsigned 8-bit given the
  [`Encoding`]; the WAV writer in the render tool defaults to signed input and
  11111 Hz output.
- A separate FM/OPL music path in the WAD (alternating register/data writes
  starting with command `0x41`) is unrelated to these samples.

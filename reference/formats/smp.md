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
- **22222 Hz**. The level engine sets the rate with the classic SB time constant:
  DSP command `0x40` then value `0xD3` (211), and `rate = 1000000 / (256 - 211) =
  22222`. This is the canonical "22 kHz" SB rate; 22050 can't be expressed as a
  time constant. Found in `LEVEL_1.WAD` at the single-cycle DMA playback routine
  (`0x40` time constant, `0xD1` speaker on, `0x14` 8-bit DMA).

## Notes

- The decoder (`smp::decode`) normalises to unsigned 8-bit given the
  [`Encoding`]; the WAV writer in the render tool defaults to signed input and
  22222 Hz output.
- A separate FM/OPL music path in the WAD (alternating register/data writes
  starting with command `0x41`) is unrelated to these samples.

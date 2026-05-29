# BDY: compressed image body

Status: verified against three real files.

## Layout

A bare IFF ILBM BODY payload, with no FORM/BMHD/CMAP wrapper (the file starts
straight into compressed data, confirmed by hex dump of `COVER3.BDY`). So the
file carries no dimensions; the caller must know them.

Compression is **ByteRun1** (PackBits). Each control byte, read as signed:

- `0..=127`: copy the next `n + 1` bytes literally.
- `-127..=-1`: repeat the next byte `1 - n` times.
- `-128`: no-op.

The decompressed bytes are **chunky 8bpp** (one palette index per pixel), not
planar. Each `.BDY` pairs with a same-named `.PAL`.

## Evidence

Decoded sizes (via the ByteRun1 unpack), all 320 pixels wide:

| File          | Compressed | Decoded | Dimensions |
|---------------|-----------:|--------:|------------|
| NEO.BDY       |     12,706 |  64,000 | 320 x 200  |
| SURPLOGO.BDY  |      6,990 |  64,000 | 320 x 200  |
| COVER3.BDY    |     70,494 | 152,960 | 320 x 478  |

NEO and SURPLOGO render as the studio logos; COVER3 (320 x 478, a tall portrait)
is the "PROTOTYPE" title cover art. All render with correct colours and no
horizontal skew, which confirms both the 320 width and the chunky layout.

## Open

- Width is 320 for every BDY seen so far. Unconfirmed whether any BDY uses a
  different width.
- COVER3's 478 height has no obvious source in the file; it has to be known
  from the code that loads it.

# SP1-4: level backgrounds

Status: verified against four level sets.

## Layout

A level's background is one 640x160 256-color image (two screens wide),
stored across four files as VGA "Mode X" byte planes:

- Plane `p` holds the pixels where `x % 4 == p`.
- `SP1` is plane 0, `SP2` plane 1, `SP3` plane 2, `SP4` plane 3.
- Each plane file is `(640 / 4) * 160` = 25600 bytes, no header.

To rebuild the image: pixel `(x, y)` = `plane[x % 4][y * 160 + x / 4]`, where
each plane is read as 160 wide by 160 tall.

The data is uncompressed indexed pixels. The palette is not in these files; it
comes from the level (the `.WAD`).

## Evidence

Every `.SP[1-4]` file is exactly 25600 bytes. Combining the four planes at
640x160 produces a coherent scene for CANYON (canyon vista), WALD (forest),
RACEB2 and ALIENBG. Reading a single file linearly, or combining at 320x320,
gives sheared noise.

This corrects the 2013 developer mail, which guessed SP1-4 were "4-layer"
(four separate parallax layers). They are four planes of one image.

## Open

- Width 640 and height 160 are constant across every set seen so far.
- Colors are unverified until a level `.WAD` palette is decoded; rendered so
  far only against a grayscale ramp and unrelated palettes.

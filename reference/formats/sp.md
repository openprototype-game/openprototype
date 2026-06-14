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

The data is uncompressed indexed pixels. Each plane is read as 160 bytes wide
by 160 rows tall (the `y * 160 + x / 4` stride above). The palette is not in
these files; it comes from the level `.WAD` (see [wad.md](wad.md)).

## Evidence

Every `.SP[1-4]` file is exactly 25600 bytes. Combining the four planes at
640x160 produces a coherent scene for CANYON (canyon vista), WALD (forest),
RACEB2, and ALIENBG, each in its level's own palette. Reading a single file
linearly, or combining at 320x320, gives sheared noise. The four files are the
four Mode X planes of one image, not four separate parallax layers.

## Parallax

The level cuts this image into horizontal strips and scrolls each at its own
rate; that scroll model, the per-level strip tables, and the vertical camera pan
are in [../render-pipeline.md](../render-pipeline.md).

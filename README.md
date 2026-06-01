# OpenPrototype

An open-source, from-scratch port of *Prototype* (1995), a DOS shoot-'em-up by
NEO Software, to Rust. The original source is lost, so the port is built by
reverse engineering the shipped game files, guided by format notes from Erik
Pojar, who programmed the original. The name follows OpenTyrian and the other
Open* remakes.

## Background

I started on this in 2013. I reached out to Erik Pojar, the original programmer,
who was kind enough to answer questions about the file formats, and then spent
months disassembling the game by hand in Ghidra. I got the pieces working in
isolation: FLI playback, images with their palettes, sound samples. But never
anything that actually ran, and I'd come back every so often only to stall in
the same spot.

In December 2025 I picked it up again, this time with AI. GitHub Copilot
couldn't carry the reverse engineering. A few months later I pointed the latest
Opus model at it, and that worked: the format decoders, the disc reader, and a
front-end that runs followed from there.

## The original game

*Prototype* was made by NEO Software in 1995:

- Programming: Erik Pojar
- Graphics: Michael Sormann, Peter Baustaedter
- Music: NEO Project, Hannes Seifert, Peter Melchart
- Design: Michael Sormann, Niki Laber
- Additional coding: Christoph Soukup, Peter Melchart
- Testing: Michaela Steurer, Victor Metyko, Niki Ghalustian, Kaweh Kazemi

Thanks especially to Erik Pojar, whose format notes made this port possible.

## Layout

- `reference/formats/`: per-format findings as they get verified.
- `crates/disc/`: library that reads game files and the OST from the CD image.
- `crates/formats/`: library of decoders for every on-disk format.
- `crates/game/`: the `openprototype` binary (the game itself).
- `crates/tools/`: CLIs for inspecting and extracting assets.
- `crates/integration-tests/`: decodes real assets sourced from the CD image
  (gated behind the `disc-tests` feature; see below).

## The disc image

The game shipped as a mixed-mode bin/cue CD: an ISO9660 data track with the game
files plus CD-DA tracks holding the soundtrack. Rather than bundle the
copyrighted files, the port reads them straight off the original image, which
you supply yourself. Download it from the Internet Archive
([archive.org/details/prototype-1995](https://archive.org/details/prototype-1995))
and drop `PROTOTYPE.bin` / `PROTOTYPE.cue` at the repo root (they are
git-ignored). `crates/disc/` reads both the files and the original-quality OST
from it; see `reference/formats/disc.md`. Set `$PROTOTYPE_DISC` to point the
tools at a cue elsewhere.

Tests that need the image are gated behind a `disc-tests` feature and are
`#[ignore]`d without it, so a plain `cargo test` skips them honestly (it reports
them as ignored, never as passed). Run them with the image in place:

```
cargo test --workspace --features disc-tests
```

## Status

Early, but the front-end runs from the disc image: the intro sequence, the main
menu, and the music jukebox. The level engine, the actual gameplay, is the next
big piece.

## Possible future extras

Not needed for faithfulness, but might happen later:

- A scaler menu in the style of DOSBox: pick between nearest, sharp-bilinear,
  and pixel-art scalers (hqx, scaleNx) at runtime. The renderer already isolates
  scaling in its own pass, so this only swaps that pass.
- A square-pixel display toggle (16:10) alongside the period-correct 4:3, for
  people who prefer pixel-perfect over the original aspect.

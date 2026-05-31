# Prototype

A from-scratch port of *Prototype* (1995), a DOS shoot-'em-up by Erik Pojar, to
Rust. The original source is lost, so the port is built by reverse engineering
the shipped game files, guided by notes from the original developer.

## Layout

- `reference/formats/` — per-format findings as they get verified.
- `crates/disc/` — library: reads game files and the OST from the CD image.
- `crates/formats/` — library: decoders for every on-disk format.
- `crates/game/` — the game binary.
- `crates/tools/` — CLIs for inspecting and extracting assets.
- `crates/integration-tests/` — decodes real assets sourced from the CD image
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

Early. Reverse engineering and format decoding come first.

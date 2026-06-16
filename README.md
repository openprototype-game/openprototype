# OpenPrototype

An open-source, from-scratch port of *Prototype* (1995), a DOS shoot-'em-up by
NEO Software, to Rust. The original source is lost, so the port is built by
reverse engineering the shipped game files, guided by format notes from Erik
Pojar, who programmed the original. The name follows OpenTyrian and the other
Open* remakes.

It plays from start to finish: all seven levels, enemies, bosses, and the
front-end, running off your own copy of the disc image.

## Install

One command downloads the latest release, fetches the disc image if you don't
already have it, and registers the desktop launcher.

Linux and macOS:

```
curl -fsSL https://raw.githubusercontent.com/openprototype-game/openprototype/main/install.sh | sh
```

Windows (PowerShell):

```
irm https://raw.githubusercontent.com/openprototype-game/openprototype/main/install.ps1 | iex
```

The script asks before pulling the ~270 MB disc image from the Internet Archive.
Already have it? Point the script at your own cue with `--cue
/path/to/PROTOTYPE.cue` (or `-Cue` on Windows) and it skips the download.
Re-running installs the latest release over the old one; `openprototype
uninstall` removes everything.

On macOS the build isn't signed yet, so the first launch needs a right-click ->
Open to get past Gatekeeper.

## Controls

In the menus (main menu, jukebox, in-game menu):

- Up / Down: move the selection
- Enter: choose the item
- Esc: back out (resume, from the in-game menu)

Flying a level:

- Arrow keys: fly the ship
- Ctrl: fire
- Shift: switch weapon
- Space: smart bomb
- Esc: open the in-game menu

Any time:

- Alt+Enter: toggle fullscreen

## Background

I started on this in 2013. I reached out to Erik Pojar, the original programmer,
who was kind enough to answer questions about the file formats, and then spent
months disassembling the game by hand in Ghidra. I got the pieces working in
isolation: FLI playback, images with their palettes, sound samples. But never
anything that actually ran, and I'd come back every so often only to stall in
the same spot.

In December 2025 I picked it up again, this time with AI. GitHub Copilot
couldn't carry the reverse engineering. A few months later I pointed the latest
Opus model at it, and that worked. The format decoders, the disc reader, the
front-end, and the level engine followed from there: this time it actually runs,
all the way through.

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

- `reference/`: verified reverse-engineering findings (the on-disk formats, the
  render pipeline, combat, audio, and more).
- `crates/formats/`: library of decoders for every on-disk format.
- `crates/disc/`: library that reads game files and the OST from the CD image.
- `crates/core/`: the platform-independent game logic and state, with no window
  or audio dependencies, so it stays headless-testable.
- `crates/backend/`: the desktop backend, the window, GPU rendering, and audio.
- `crates/game/`: the `openprototype` binary (the game itself).
- `crates/install/`: per-user desktop install and uninstall (the launcher entry
  and the icon decoded from the disc).
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
tools at a cue elsewhere. The installer above can fetch the same image for you.

Tests that need the image are gated behind a `disc-tests` feature and are
`#[ignore]`d without it, so a plain `cargo test` skips them honestly (it reports
them as ignored, never as passed). Run them with the image in place:

```
cargo test --workspace --features disc-tests
```

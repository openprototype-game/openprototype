# Prototype

A from-scratch port of *Prototype* (1995), a DOS shoot-'em-up by Erik Pojar, to
Rust. The original source is lost, so the port is built by reverse engineering
the shipped game files, guided by notes from the original developer.

## Layout

- `reference/formats/` — per-format findings as they get verified.
- `crates/formats/` — library: decoders for every on-disk format.
- `crates/game/` — the game binary.
- `crates/tools/` — CLIs for inspecting and extracting assets.

## Status

Early. Reverse engineering and format decoding come first.

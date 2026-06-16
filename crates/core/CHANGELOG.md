# Changelog

## [0.2.0](https://github.com/openprototype-game/openprototype/compare/v0.1.0...v0.2.0) (2026-06-16)


### Features

* **core:** add a transparency-aware framebuffer blit ([6dc01ab](https://github.com/openprototype-game/openprototype/commit/6dc01abcb2889353f980ec03eef6fdc70fe0bea5))
* **core:** add GameState rules and bounded counters ([d753bbc](https://github.com/openprototype-game/openprototype/commit/d753bbc982ce57841639c3befeeff3e0d55ac9ec))
* **frontend:** run the full game-over sequence from the level ([a20670e](https://github.com/openprototype-game/openprototype/commit/a20670e381df6cc2c747095a48a86a5f339f456a))
* **game:** chain the levels behind NEW GAME ([b452dc0](https://github.com/openprototype-game/openprototype/commit/b452dc0c8f93ef8fe3bf5d2a3be8c86f91b64882))
* **game:** fly the player ship with its shield and camera coupling ([9833f8b](https://github.com/openprototype-game/openprototype/commit/9833f8b8e8d64936635e489cd0a43a8b8011ff1a))
* **game:** play the level sound effects ([e93318e](https://github.com/openprototype-game/openprototype/commit/e93318e2ab20c5d6fdc6a1ebdce955c568072a89))
* **game:** render the in-game HUD panel with score and lives ([27486e1](https://github.com/openprototype-game/openprototype/commit/27486e1d2f64c06a0031f3dbaf51cfc0c0aa5367))
* **game:** switch weapons with Shift and add dev charge-level keys ([ab9f28e](https://github.com/openprototype-game/openprototype/commit/ab9f28e3927b80703995a3aace5de6d234d1126f))
* **level:** port the race levels' spawn consumer and game mode ([e27f31e](https://github.com/openprototype-game/openprototype/commit/e27f31eacb17a140ae1798d865666535930423e3))
* **level:** wire the in-game VOLUME submenu ([bc74922](https://github.com/openprototype-game/openprototype/commit/bc74922c918136c0f0f5e4162fda15cd109aecff))


### Bug Fixes

* **audio:** let asteroid explosions through the chaingun's impact stream ([c7fff84](https://github.com/openprototype-game/openprototype/commit/c7fff847991f2fb9c09c36adf0c93300fb467c7b))
* **audio:** play the level samples at the engine's real 11111 Hz ([9abcdb7](https://github.com/openprototype-game/openprototype/commit/9abcdb7163ac256dd050123dcce6b7884952c275))
* **backend:** drive each scene on a fixed timestep at its own refresh ([30970e6](https://github.com/openprototype-game/openprototype/commit/30970e627325a203db7c0e671a8fe76c945aa8c4))
* **combat:** drain the firing weapon on hits, not the selection ([14a8054](https://github.com/openprototype-game/openprototype/commit/14a8054ada0cffba0ee532228a3a79bd389ec9fc))
* **core:** grant at most one extra life per score check ([259142c](https://github.com/openprototype-game/openprototype/commit/259142ca2d9da74635027a37ccc08c23159b4c04))
* **level:** deduct the life when the death sequence ends, not at the hit ([8a2b448](https://github.com/openprototype-game/openprototype/commit/8a2b448d93a20083875151d0ac3c2d6442cf5b6c))
* **sfx:** fire the orb-conversion sound and the extra-life jingle ([83ec863](https://github.com/openprototype-game/openprototype/commit/83ec863904ee411810d9120c9008d0a01a167a7d))

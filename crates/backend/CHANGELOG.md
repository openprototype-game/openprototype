# Changelog

## [0.2.0](https://github.com/openprototype-game/openprototype/compare/v0.1.0...v0.2.0) (2026-06-16)


### Features

* **backend:** emit tracing events instead of stderr prints ([5fcae42](https://github.com/openprototype-game/openprototype/commit/5fcae424832e20d3f58089f7688c68af2908c623))
* **backend:** present with Mailbox where the surface supports it ([250fafc](https://github.com/openprototype-game/openprototype/commit/250fafc58d1731731872e89c7eb01beac2b412cd))
* **backend:** wgpu renderer with GPU palette expansion and 4:3 scaling ([407cc8a](https://github.com/openprototype-game/openprototype/commit/407cc8ad11c6c0909a35c93ba3cf8ee41f4aa83e))
* **frontend:** run the full game-over sequence from the level ([a20670e](https://github.com/openprototype-game/openprototype/commit/a20670e381df6cc2c747095a48a86a5f339f456a))
* **game:** fly the player ship with its shield and camera coupling ([9833f8b](https://github.com/openprototype-game/openprototype/commit/9833f8b8e8d64936635e489cd0a43a8b8011ff1a))
* **game:** play the level sound effects ([e93318e](https://github.com/openprototype-game/openprototype/commit/e93318e2ab20c5d6fdc6a1ebdce955c568072a89))
* **game:** switch weapons with Shift and add dev charge-level keys ([ab9f28e](https://github.com/openprototype-game/openprototype/commit/ab9f28e3927b80703995a3aace5de6d234d1126f))
* **level:** wire the in-game VOLUME submenu ([bc74922](https://github.com/openprototype-game/openprototype/commit/bc74922c918136c0f0f5e4162fda15cd109aecff))
* **window:** set a stable WM_CLASS / app_id ([fdcab51](https://github.com/openprototype-game/openprototype/commit/fdcab5105b0e63936f1972536aeaa7ea35e3e737))
* **window:** title OpenPrototype and a ship window icon ([16fb6bc](https://github.com/openprototype-game/openprototype/commit/16fb6bc2d92ab39b501529f02901256c419aff5d))


### Bug Fixes

* **audio:** let asteroid explosions through the chaingun's impact stream ([c7fff84](https://github.com/openprototype-game/openprototype/commit/c7fff847991f2fb9c09c36adf0c93300fb467c7b))
* **audio:** play the level samples at the engine's real 11111 Hz ([9abcdb7](https://github.com/openprototype-game/openprototype/commit/9abcdb7163ac256dd050123dcce6b7884952c275))
* **backend:** deliver the spacebar to the game ([5644252](https://github.com/openprototype-game/openprototype/commit/5644252644beb74268b8ca58651274269e1e1517))
* **backend:** drive each scene on a fixed timestep at its own refresh ([30970e6](https://github.com/openprototype-game/openprototype/commit/30970e627325a203db7c0e671a8fe76c945aa8c4))
* **backend:** keep logic time on the wall clock ([ccec7cf](https://github.com/openprototype-game/openprototype/commit/ccec7cfecb40d26b22d0c0086610995400c1c05f))

# Changelog

## [0.2.0](https://github.com/openprototype-game/openprototype/compare/v0.1.0...v0.2.0) (2026-06-16)


### Features

* **core:** add GameState rules and bounded counters ([d753bbc](https://github.com/openprototype-game/openprototype/commit/d753bbc982ce57841639c3befeeff3e0d55ac9ec))
* **disc:** read game files and OST from the CD image ([587fede](https://github.com/openprototype-game/openprototype/commit/587fedee1ace9dd39e257e4ffec7dbccf344763d))
* **formats:** add bitmap font decoder and menu preview ([b37ce51](https://github.com/openprototype-game/openprototype/commit/b37ce517423b284c90546f606ea364144c4d9044))
* **formats:** add FLI animation decoder ([acb5fc4](https://github.com/openprototype-game/openprototype/commit/acb5fc458523f6088beecf266939ff262b8a75fd))
* **formats:** add SMP sound decoder with WAV export ([9fbddda](https://github.com/openprototype-game/openprototype/commit/9fbddda5cb44dfddb24666f1a06eabe303b3d0c5))
* **formats:** add START.EXE reader for the menu palette ([dfc4f03](https://github.com/openprototype-game/openprototype/commit/dfc4f03fbb589466e7b5cec8e6ba52d73bbcd746))
* **formats:** decode SP1-4 level backgrounds (Mode X planes) ([657cb9e](https://github.com/openprototype-game/openprototype/commit/657cb9e229756df846def853c166c3f83450df66))
* **formats:** implement BDY ByteRun1 decoder ([1325134](https://github.com/openprototype-game/openprototype/commit/1325134056e62a03e0e8e1fea8f91005969b14f5))
* **game:** add a per-level data registry and route asset loading through it ([463cb23](https://github.com/openprototype-game/openprototype/commit/463cb2391e6a1273b9f24efc1acc56fb942b8f4b))
* **game:** draw the weapon pod on the HUD panel ([dc0328b](https://github.com/openprototype-game/openprototype/commit/dc0328b37aa10e1ae23f6e71fb58b93f3d9e21a3))
* **game:** fly the player ship with its shield and camera coupling ([9833f8b](https://github.com/openprototype-game/openprototype/commit/9833f8b8e8d64936635e489cd0a43a8b8011ff1a))
* **game:** render the in-game HUD panel with score and lives ([27486e1](https://github.com/openprototype-game/openprototype/commit/27486e1d2f64c06a0031f3dbaf51cfc0c0aa5367))
* **level:** port the race levels' spawn consumer and game mode ([e27f31e](https://github.com/openprototype-game/openprototype/commit/e27f31eacb17a140ae1798d865666535930423e3))
* **tools:** add 'render bin' sprite mode ([bbb00af](https://github.com/openprototype-game/openprototype/commit/bbb00af966c44d0b0a4ef9b76333f146a8380685))
* **tools:** add render CLI to dump PAL/RAW assets as PNG ([f8422f7](https://github.com/openprototype-game/openprototype/commit/f8422f79ec3783af51e6f0e01f40e1a2c7f78f95))
* **tools:** render scenery BINs with the level WAD palette ([28acc97](https://github.com/openprototype-game/openprototype/commit/28acc97425e69f17ab7ff82fd924e1464825f258))


### Bug Fixes

* **audio:** play the level samples at the engine's real 11111 Hz ([9abcdb7](https://github.com/openprototype-game/openprototype/commit/9abcdb7163ac256dd050123dcce6b7884952c275))
* **game:** render the level in its real 320x160 mode ([063d0d1](https://github.com/openprototype-game/openprototype/commit/063d0d191d8de0fb82dc7872672681007e0d5d74))

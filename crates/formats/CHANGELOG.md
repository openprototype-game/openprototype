# Changelog

## [0.2.0](https://github.com/openprototype-game/openprototype/compare/v0.1.0...v0.2.0) (2026-06-16)


### Features

* **backend:** wgpu renderer with GPU palette expansion and 4:3 scaling ([407cc8a](https://github.com/openprototype-game/openprototype/commit/407cc8ad11c6c0909a35c93ba3cf8ee41f4aa83e))
* **formats:** add bin compiled-sprite decoder ([b835682](https://github.com/openprototype-game/openprototype/commit/b8356828494a39fada08c699d3ae29aaf02231dd))
* **formats:** add bitmap font decoder and menu preview ([b37ce51](https://github.com/openprototype-game/openprototype/commit/b37ce517423b284c90546f606ea364144c4d9044))
* **formats:** add core data model and PAL/RAW decoders ([293e6b6](https://github.com/openprototype-game/openprototype/commit/293e6b601edc6d2cb7ba906e31b16a51ac886f21))
* **formats:** add FLI animation decoder ([acb5fc4](https://github.com/openprototype-game/openprototype/commit/acb5fc458523f6088beecf266939ff262b8a75fd))
* **formats:** add HIGH.TXT high-score table decoder ([b0c3239](https://github.com/openprototype-game/openprototype/commit/b0c323985d7caea2888f095b19409e1cbc182517))
* **formats:** add SMP sound decoder with WAV export ([9fbddda](https://github.com/openprototype-game/openprototype/commit/9fbddda5cb44dfddb24666f1a06eabe303b3d0c5))
* **formats:** add START.EXE reader for the menu palette ([dfc4f03](https://github.com/openprototype-game/openprototype/commit/dfc4f03fbb589466e7b5cec8e6ba52d73bbcd746))
* **formats:** decode banked catalogs with the consumer's plane addressing ([514d836](https://github.com/openprototype-game/openprototype/commit/514d836f39a2a9e6a45b2f6be31e16a5398f548e))
* **formats:** decode SP1-4 level backgrounds (Mode X planes) ([657cb9e](https://github.com/openprototype-game/openprototype/commit/657cb9e229756df846def853c166c3f83450df66))
* **formats:** implement BDY ByteRun1 decoder ([1325134](https://github.com/openprototype-game/openprototype/commit/1325134056e62a03e0e8e1fea8f91005969b14f5))
* **formats:** record each decoded sprite's cell origin ([3e62553](https://github.com/openprototype-game/openprototype/commit/3e625531ef508921bf536ee0e917755824cb896a))
* **game:** port the in-game Esc menu with its save and load slots ([59ee5b2](https://github.com/openprototype-game/openprototype/commit/59ee5b2b702567a6b742b2a6fded0defec770cd0))
* **game:** render the in-game HUD panel with score and lives ([27486e1](https://github.com/openprototype-game/openprototype/commit/27486e1d2f64c06a0031f3dbaf51cfc0c0aa5367))
* **level:** port the invincibility wear-off palette fade ([eb11f0e](https://github.com/openprototype-game/openprototype/commit/eb11f0efa979ce147412390e1dc6280f3d9ed0df))


### Bug Fixes

* **formats:** decode the FLI COPY chunk with its 2-byte overread ([3a26fdb](https://github.com/openprototype-game/openprototype/commit/3a26fdb08226c6edde820c59e09d67f2a3413c34))

# uSVC emulator, SDK and games

Tools for writing games for the [uSVC](https://github.com/next-hack/uSVC)
console (uChip Simple VGA Console, ATSAMD21 Cortex-M0+) without the hardware,
and a game made with them.

- **Emulator** (`crates/`): runs unmodified uSVC game packages (`.usc`) in a
  window with sound, keyboard and gamepad, or headless for automated tests.
- **SDK** (`sdk/`): builds games with `arm-none-eabi-gcc` against the
  upstream kernel.
- **Tools** (`tools/`): package (`usc.py`), artwork (`gfx.py`) and music
  (`song.py`) converters.
- **Shardbreaker** (`games/shardbreaker`): a brick-breaking game.
  See its [README](games/shardbreaker/README.md).

| | |
|---|---|
| ![Shardbreaker title screen](docs/images/shardbreaker-title.png) | ![Laser paddle on level 1](docs/images/shardbreaker-laser.png) |
| ![Catch paddle and a falling capsule on level 2](docs/images/shardbreaker-chevrons.png) | ![Level 4 with a wisp and a capsule](docs/images/shardbreaker-diamond.png) |

Shardbreaker running in the emulator: 320x200 pixels, 256 colours, tiles and
sprites drawn by the console's own kernel.

Nothing here has been tested on a real console yet.

## Getting started

Needs Rust, SDL2, `uv`, and for building games the Arm GNU toolchain
(`brew install sdl2 uv` and `brew install --cask gcc-arm-embedded` on macOS).

```sh
git submodule update --init --depth 1
cargo build --release

# play a shipped game
./target/release/usvc "reference/uSVC/usc packages/Tetris.usc"

# build and play Shardbreaker
make -C sdk GAME=../games/shardbreaker play
```

`CLAUDE.md` lists the commands for development and testing.

## Licence

Copyright (C) 2026 Michael Wulff Nielsen.

This program is free software: you can redistribute it and/or modify it under
the terms of the GNU General Public License as published by the Free Software
Foundation, either version 3 of the License, or (at your option) any later
version. It is distributed without any warranty. See [LICENSE](LICENSE).

That covers the emulator, the SDK files, the tools, and the games including
their artwork, levels and music.

### Other people's work

- **uSVC kernel, game loader and example games** by Nicola Wrachien
  (next-hack.com), GPL-3.0-or-later. They are git submodules under
  `reference/` and are not copied into this repository. A built game package
  contains the kernel, which is why games must be GPL too. The kernel in turn
  includes the Uzebox sound engine by Alec Bourque (GPL), `printf` by Marco
  Paland (MIT), Petit FatFs by ChaN, and Microchip device headers
  (Apache-2.0).
- **CMSIS core headers** in `sdk/cmsis/`, Copyright Arm Limited, Apache-2.0
  (`sdk/cmsis/LICENSE.txt`).
- The keyboard translation in `crates/usvc-core/src/hle.rs` is ported from
  the uSVC kernel.

uSVC and uChip are names of their respective owners; this project is not
affiliated with them.

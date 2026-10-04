# uSVC emulator

Emulator and SDK for the uSVC console (ATSAMD21E18, Cortex-M0+, VGA), so games
can be written and tested without hardware. Plan and hardware notes:
`docs/PLAN.md`.

## Layout

- `crates/usvc-core` – portable emulator core. No I/O, no platform
  dependencies, no crates. Keep it that way.
- `crates/usvc-cli` – the `usvc` binary: an SDL window by default,
  `--headless` for automated runs. `--no-default-features` builds it
  without SDL.
- `tools/` – Python utilities, managed with `uv`.
- `sdk/` – Makefile, CMSIS headers and `usvc_debug.h` for building games.
- `games/` – our games. `games/hello` is the minimal template;
  `games/shardbreaker` is a full game (8bpp tiles, sprites, sound, keyboard
  and gamepad) and the model for asset handling.
- `reference/` – upstream next-hack repos as submodules. Read-only.
  `reference/uSVC/usc packages/*.usc` are the test programs.

## Commands

```sh
cargo build --release
cargo test --release            # CPU unit tests + all shipped games
cargo clippy --release --all-targets

# run a game for N frames and look at the result
./target/release/usvc "reference/uSVC/usc packages/Tetris.usc" --headless \
    --frames 300 --png out/shot.png --tap 100:S --tap 160:SPACE -v

# build a game and run it in the emulator. Output goes to build/<name>-emu/
# with EMULATOR=1 and build/<name>/ without (the package for real hardware).
make -C sdk GAME=../games/hello EMULATOR=1 run FRAMES=120 RUNFLAGS="--tap 60:K"

cd tools && uv run pytest -q && uv run ruff check . && uv run ruff format .
uv run usc.py info GAME.usc
```

Always pass `--headless` when running it yourself; without it a window
opens on the user's screen and runs until closed (or until `--frames`).
`make ... run` is headless, `make ... play` opens the window.

`usvc --help` lists the options: `--no-keyboard`, `--scale`, `--mute`, `--png`, `--png-every N`, `--wav`,
`--input SCRIPT`, `--tap FRAME:KEY`, `--gamepad`, `--report FILE.json`,
`--lss LISTING` (symbol names for faults).

## Things to know

- Source files under `reference/` are ISO-8859; use `LC_ALL=C grep -a`.
- The run summary prints `min_wfi_slack`. Zero means the scanline interrupt
  reached its `WFI` late and the picture is shifted: a timing bug.
- A healthy frame has `lines_drawn` = 400 per frame.
- Tetris is played with W/A/S/D and space, not the arrow keys.
- In the window the host keyboard is the console's USB keyboard and the
  first host gamepad or joystick is its USB gamepad. F12 saves a screenshot.
- Tetris and other games use the gamepad only when no keyboard is
  installed; pass `--no-keyboard` to test gamepad input.
- A game is a directory with `main.c`, `main.h` and `usvc_config.h`; the
  Makefile adds the kernel from `reference/` (or the game's own
  `usvc_kernel/` if it has one). Never edit files under `reference/`.
- Artwork: PNG files in a game's `assets/` are converted to C in `gen/` by
  `tools/gfx.py` (`make -C sdk GAME=... assets`). `gen/` is committed.
  Sprite pixel value 0 is transparent, so the tool stores opaque black as
  the darkest red.
- Music: songs are text files in a game's `music/` directory, converted by
  `tools/song.py` (also run by the `assets` target). Channel 3 is left to
  sound effects. Nobody can hear the result headless; ask the user.
- In 8bpp tile mode every tile shown must be in RAM (`tiles[]`), 64 bytes
  each, and each tile a sprite overlaps costs another; watch the 32 KB.
- Change tiles only between `restoreBackgroundTiles()` and `drawSprites()`,
  inside the vertical blank. Shardbreaker's `EMULATOR=1` build prints the
  scan line where a frame's work ended; it must stay below 524.
- `--tap FRAME:KEY:FRAMES` holds a key. Shardbreaker plays itself if the
  title screen is left alone for 900 frames, which is the easy way to
  exercise it headless. Its `EMULATOR=1` build also accepts keys 1-6
  during play to hand out a capsule (E S M L Z C), 9 to skip a level and 0
  to toggle an autopilot.
- `usvcDebugPrint()` (from `usvc_debug.h`) writes text to the emulator's
  stdout. It only does anything when built with `EMULATOR=1`; the port
  address faults on real hardware, so hardware builds must omit that flag.
- The four "not implemented and will always fail" linker warnings are
  expected.
- Check after kernel-affecting changes: upstream Tetris rebuilt with
  `make -C sdk GAME=../reference/uSVC_Tetris NAME=tetris FRAMES=200 run`
  must print the same `hash=` as the shipped `Tetris.usc` at 200 frames.
- Screenshots for the READMEs are in `docs/images/`. `--png` writes 320x400;
  double the width (nearest neighbour) to get the shape the console shows.
- The golden frame hashes in `crates/usvc-core/tests/games.rs` were approved
  by eye from screenshots, not checked against real hardware. Only change
  one after the user has approved the new picture.

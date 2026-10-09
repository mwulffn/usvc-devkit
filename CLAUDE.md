# usvc-devkit

Emulator, SDK and tools for the uSVC console (ATSAMD21E18, Cortex-M0+, VGA),
and a game made with them. This file holds commands and conventions for
working in the repository. The substance is in `docs/`:

- `docs/hardware.md` – how the console and kernel work
- `docs/emulator.md` – the `usvc` program, its options and summary fields,
  what is and is not emulated
- `docs/making-a-game.md` – the frame loop, the rules that are easy to
  break, artwork, music, input, packaging, testing

## Layout

- `crates/usvc-core` – portable emulator core. No I/O, no platform
  dependencies, no crates. Keep it that way.
- `crates/usvc-cli` – the `usvc` binary: an SDL window by default,
  `--headless` for automated runs. `--no-default-features` builds it
  without SDL.
- `tools/` – Python utilities, managed with `uv`.
- `sdk/` – Makefile, CMSIS headers and `usvc_debug.h` for building games.
- `loader/` – builds the console's game loader from the upstream source.
  With `HUB=1` it builds a loader with USB hub support and no logo or sound
  (`loader/hub`: one new file and patches to a copy of the upstream source).
  `loader/hubtest` is a test program that runs in the loader's place on a
  real console and shows what it finds behind a USB hub. All of these only
  build; nothing in the repository writes to a console.
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

# build the game loader and report how much of its 16384-byte slot is free.
# HUB=1 builds the one with USB hub support, in build/loader-hub/
make -C loader
make -C loader HUB=1

cd tools && uv run pytest -q && uv run ruff check . && uv run ruff format .
uv run usc.py info GAME.usc
```

Always pass `--headless` when running it yourself; without it a window
opens on the user's screen and runs until closed (or until `--frames`).
`make ... run` is headless, `make ... play` opens the window.

## Things to know

- Source files under `reference/` are ISO-8859; use `LC_ALL=C grep -a`.
  Never edit them.
- Keep `crates/usvc-core` free of I/O and dependencies.
- In a run summary, `lines_drawn` must be 400 per frame and `min_wfi_slack`
  above zero.
- Tetris is played with W/A/S/D and space. Tetris reads the gamepad only
  when no keyboard is installed (`--no-keyboard`); Redballs, Fairplay Race
  and the horse demo let a plugged-in gamepad override the keyboard.
- Nobody can hear sound or music in a headless run; ask the user to listen.
- Shardbreaker plays itself if its title screen is left alone for 900
  frames. Its `EMULATOR=1` build accepts keys 1-6 during play to hand out a
  capsule (E S M L Z C), 9 to clear the level and 0 to toggle an autopilot.
- After a change that could affect the kernel build, upstream Tetris rebuilt
  with `make -C sdk GAME=../reference/uSVC_Tetris NAME=tetris FRAMES=200 run`
  must print the same `hash=` as the shipped `Tetris.usc` at 200 frames.
- Screenshots for the READMEs are in `docs/images/`, at double width
  (640x400).
- The golden frame hashes in `crates/usvc-core/tests/games.rs` were approved
  by eye from screenshots, not checked against real hardware. Only change
  one after the user has approved the new picture.
- When behaviour changes, update the matching file in `docs/`.

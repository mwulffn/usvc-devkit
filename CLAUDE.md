# uSVC emulator

Emulator and SDK for the uSVC console (ATSAMD21E18, Cortex-M0+, VGA), so games
can be written and tested without hardware. Plan and hardware notes:
`docs/PLAN.md`.

## Layout

- `crates/usvc-core` – portable emulator core. No I/O, no platform
  dependencies, no crates. Keep it that way.
- `crates/usvc-cli` – headless runner (`usvc` binary).
- `tools/` – Python utilities, managed with `uv`.
- `sdk/` – Makefile, CMSIS headers and `usvc_debug.h` for building games.
- `games/` – our games. `games/hello` is the template to copy.
- `reference/` – upstream next-hack repos as submodules. Read-only.
  `reference/uSVC/usc packages/*.usc` are the test programs.

## Commands

```sh
cargo build --release
cargo test --release            # CPU unit tests + all shipped games
cargo clippy --release --all-targets

# run a game for N frames and look at the result
./target/release/usvc "reference/uSVC/usc packages/Tetris.usc" \
    --frames 300 --png out/shot.png --tap 100:S --tap 160:SPACE -v

# build a game and run it in the emulator (output in build/<name>/)
make -C sdk GAME=../games/hello EMULATOR=1 run FRAMES=120 RUNFLAGS="--tap 60:K"

cd tools && uv run pytest -q && uv run ruff check . && uv run ruff format .
uv run usc.py info GAME.usc
```

`usvc --help` lists the options: `--png`, `--png-every N`, `--wav`,
`--input SCRIPT`, `--tap FRAME:KEY`, `--gamepad`, `--report FILE.json`,
`--lss LISTING` (symbol names for faults).

## Things to know

- Source files under `reference/` are ISO-8859; use `LC_ALL=C grep -a`.
- The run summary prints `min_wfi_slack`. Zero means the scanline interrupt
  reached its `WFI` late and the picture is shifted: a timing bug.
- A healthy frame has `lines_drawn` = 400 per frame.
- Tetris is played with W/A/S/D and space, not the arrow keys.
- A game is a directory with `main.c`, `main.h` and `usvc_config.h`; the
  Makefile adds the kernel from `reference/` (or the game's own
  `usvc_kernel/` if it has one). Never edit files under `reference/`.
- `usvcDebugPrint()` (from `usvc_debug.h`) writes text to the emulator's
  stdout. It only does anything when built with `EMULATOR=1`; the port
  address faults on real hardware, so hardware builds must omit that flag.
- The four "not implemented and will always fail" linker warnings are
  expected.
- Check after kernel-affecting changes: upstream Tetris rebuilt with
  `make -C sdk GAME=../reference/uSVC_Tetris NAME=tetris FRAMES=200 run`
  must print the same `hash=` as the shipped `Tetris.usc` at 200 frames.
- The golden frame hashes in `crates/usvc-core/tests/games.rs` were approved
  by eye from screenshots, not checked against real hardware. Only change
  one after the user has approved the new picture.

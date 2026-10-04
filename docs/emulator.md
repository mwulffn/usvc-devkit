# The emulator

`usvc` runs unmodified uSVC game packages on a PC. It was written to make
game development possible without a console, and for automated testing.

Its timing is close enough to draw a correct picture, not cycle-exact, and
it has not been compared with real hardware. See "What is not emulated".

## Running it

```sh
cargo build --release
./target/release/usvc GAME.usc              # window, sound, live input
./target/release/usvc GAME.usc --headless   # no window; prints a summary
```

`GAME` is a `.usc` package or a raw binary linked at `0x6000`.

### In the window

- The host keyboard is the console's USB keyboard.
- The first host gamepad or joystick is the console's USB gamepad, with
  hot-plug. Recognised controllers are mapped to the kernel's button names
  (A/B/X/Y to buttons 1-4, shoulders, triggers, select, start), and the
  direction pad moves the main axes to their limits. Plain joysticks pass
  buttons and axes through in report order.
- F12 saves a screenshot to `--png-dir`. Closing the window ends the run.

### Options

| Option | Effect |
|---|---|
| `--headless` | No window; run as fast as possible |
| `-f`, `--frames N` | Stop after N frames (headless default 60; window: until closed) |
| `--png FILE` | Write the last frame as a PNG |
| `--png-every N` | Write a PNG every N frames into `--png-dir` (default `out`) |
| `--wav FILE` | Write the audio |
| `--input FILE` | Input script (below) |
| `--tap FRAME:KEY[:FRAMES]` | Press a key at a frame, optionally holding it; repeatable |
| `--gamepad` | Plug in a gamepad even if the host has none |
| `--no-keyboard` | Unplug the keyboard, for games that prefer it over the gamepad |
| `--scale N`, `--mute` | Window size and sound |
| `--report FILE` | Write the run summary as JSON |
| `--lss FILE` | Listing to take symbol names from, for readable fault addresses |
| `-v` | Also print video timing, library call counts and stubbed registers |

PNG files are 320x400, the pixels the console sends. A monitor shows them at
a 640x400 shape; double the width to get that.

### Input scripts

One event per line; `#` starts a comment.

```text
60 tap ENTER          # press for 3 frames
90 tap DOWN 10        # press for 10 frames
120 press LEFT
150 release LEFT
200 pad 0x0001 0 127  # gamepad buttons, then optional X and Y axis values
```

Key names: letters, digits, `F1`-`F12`, `ENTER`, `ESC`, `SPACE`, `TAB`,
`BACKSPACE`, `DELETE`, `UP`, `DOWN`, `LEFT`, `RIGHT`, `MINUS`, `EQUAL`,
`COMMA`, `DOT`, `SLASH`, and the modifiers `LSHIFT`, `LCTRL`, `LALT` and
their right-hand versions.

### The run summary

```text
frames=2186 timeouts=0 cycles=1836272429 instructions=1359353759 hash=2cebf8f02cb368dd
interrupts=1420748 handler_share=79.2% lines_drawn=874400 min_wfi_slack=48 min_sp=0x20004788
pc=0x0000a856 (usbHostTask+0xa)
```

| Field | Meaning |
|---|---|
| `frames`, `timeouts` | Frames that ended at a vertical sync, and frames that did not (video not running) |
| `hash` | Hash of the last picture; equal hashes mean equal pictures |
| `handler_share` | Share of CPU time spent in interrupts, mostly drawing |
| `lines_drawn` | Should be 400 per frame |
| `min_wfi_slack` | Smallest margin, in cycles, with which the scanline handler reached its wait point. Zero means a line was drawn late and the picture is shifted. |
| `min_sp` | Lowest stack pointer seen at an interrupt; compare with `_sstack` in the listing |
| `pc` | Where the program was at the end |

A `FAULT` line follows if the program stopped: an access to unmapped memory,
an undefined instruction, a breakpoint, a reset request, or a `WFI` that
nothing could wake. The exit code is then 1.

## How it is built

- `crates/usvc-core` is the emulator proper. It has no dependencies, does no
  I/O and knows nothing about windows or files, so other front ends can use
  it. A front end calls `Machine::load_usc`, then `run_frame` in a loop, and
  reads `framebuffer()` and `take_audio()`.
- `crates/usvc-cli` is the `usvc` program: argument handling, PNG/WAV/JSON
  output, input scripts, and the SDL window (behind the `window` feature;
  `--no-default-features` builds without SDL).

Inside the core:

| File | Contents |
|---|---|
| `cpu.rs` | ARMv6-M Thumb interpreter with cycle counts, exceptions, interrupt priorities, `WFI` |
| `bus.rs` | Memory map, peripheral registers, system control space |
| `timers.rs` | `TCC0`-`TCC2`, the event system, SysTick; events are scheduled, not ticked |
| `video.rs` | Rebuilds the picture from writes to the port, by time since the sync signals |
| `hle.rs` | Host implementation of the bootloader library |
| `usc.rs`, `keys.rs`, `audio.rs` | Package parsing, key names, DC removal for audio |

### What is modelled

- The CPU, with the documented Cortex-M0+ cycle counts: 1 for data
  processing, 2 for loads and stores (1 on the I/O bus), `1+N` for
  multi-register transfers, 2 for a taken branch, 15 for interrupt entry.
- PORT A's output register, timers `TCC0`-`TCC2`, the three event channels
  the kernel uses, the DAC, `TC3`/`TC4` as a millisecond counter, SysTick
  and the interrupt controller.
- The picture: each write to the upper half of the port is placed by its
  cycle within the line, on a 4-cycle grid that snaps to the line's first
  write (the video modes start a few cycles apart).
- Sound: every DAC write is one sample at 30 kHz.

### The bootloader library

There is no game loader in the emulator. A small table is placed where
games expect the loader's function table, pointing at trap instructions.
When a game calls a library function, the emulator performs it on the host:

- Keyboard and gamepad functions return the host's input, converted the way
  the kernel's own drivers would.
- "Installed" callbacks that a game registers are called on its first
  `usbHostTask`.
- USB housekeeping functions do nothing.
- File functions return "not ready".

This is why no ROM or loader image is needed, and why the USB controller and
SD card are not emulated.

### What is not emulated

- **Real USB and SD card.** A game built with its own USB stack (not using
  the bootloader library) gets no input. File access does not work.
- **The game loader.** Packages start directly.
- **Flash wait states and bus stalls.** Code runs slightly faster than on
  hardware, and the kernel's comments mention bus effects it works around
  that are not reproduced. A game that barely fits its frame time here may
  not fit on a console.
- **Other peripherals.** Registers of anything not listed above keep what is
  written to them and otherwise do nothing; ready and lock flags read as
  set. `-v` lists the ones a program touched.
- **Analogue behaviour**, such as the output filter on the sound.

### The debug port

A byte written to `0x42005400` is printed on the host's standard output.
The address is unmapped on real hardware and a write there would fault, so
games must only use it in emulator builds. `sdk/include/usvc_debug.h` wraps
it; see [making-a-game.md](making-a-game.md).

## Tests

```sh
cargo test --release
```

- CPU tests on hand-assembled code: flags, cycle counts, interrupt entry and
  return, and the `WFI` wake rule.
- Each of the six upstream packages is run for 50 frames and must draw 400
  lines per frame, never reach its wait point late, give the same picture on
  two runs, and match a stored picture hash.

The stored hashes were approved by looking at screenshots. They guard
against changes, not against the emulator being wrong.

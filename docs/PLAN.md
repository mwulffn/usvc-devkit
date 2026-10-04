# uSVC emulator and SDK plan

Status: confirmed. M0-M5 are implemented except the items listed under
"Progress". M6 (window) is implemented; the user confirmed that keyboard,
gamepad, Tetris and Redballs work in it.

## Progress

- Done: CPU, bus, interrupts, timers, event system, video capture, audio
  capture, bootloader call traps with guest callbacks, input scripts, PNG,
  WAV and JSON output, `.usc` tool. All six shipped packages boot and draw
  400 lines per frame with positive `WFI` slack.
- Golden hashes for all six packages are in the game tests. They were
  approved by eye from screenshots, not compared with real hardware.
- Open in M4: a scripted Tetris session beyond the menu.
- M5: `sdk/Makefile` builds with Arm GNU Toolchain 15.3 and the upstream
  flags. The kernel compiles unmodified, the scanline handler still lands in
  RAM, and rebuilt Tetris gives the same frame hash and `WFI` slack as the
  shipped package. `games/hello` is the template. Not tested on hardware.
- Measured picture position: first visible line is 76 lines after vertical
  sync; pixel 0 is 274 (bitmapped), 277 (4bpp) or 279 (8bpp) cycles into the
  line. The rasteriser snaps each line to its first write.

## Goal

Let us write uSVC games with Claude Code and test them without hardware:
build a game, run it headless, look at a screenshot, feed it input, read
text output and a fault report. Secondary: a window with sound to play in.

Decided: custom emulator that runs unmodified game binaries, written in Rust,
with Python (`uv`) for utility scripts. "Close enough" timing, not a
cycle-exact hardware model.

## What the hardware and kernel require

Facts read from `reference/uSVC/software/uSVC_Template_Project/usvc_kernel`
(line numbers refer to that copy).

**Machine**

- ATSAMD21E18: Cortex-M0+ at 48 MHz, 256 KB flash, 32 KB RAM at `0x20000000`.
- One line is 1600 CPU cycles (30 kHz); 525 lines per frame (about 57 Hz);
  lines 0-399 are visible, 320x200 line-doubled. Odd and even lines can
  differ (hi-res row remapping and per-line colour changes in 4bpp mode).

**Line timing**

- `TCC1` free-runs with period 1600 and drives hsync. Its overflow event
  (EVSYS channel 2, asynchronous) retriggers `TCC2` and increments `TCC0`
  (line counter, vsync).
- `TCC1` compare 1 (count 1) raises the scanline interrupt, priority 1
  (`vga.c:918`, `976`). `TCC1_Handler` runs from RAM with PRIMASK set, runs
  the audio mixer, prepares the line, then executes `WFI`.
- `TCC2` compare 0 (count 268, `vga.c:938`) goes through EVSYS channel 0
  (synchronous) and sets the EVSYS interrupt pending, priority 0
  (`vga.c:893`). It wakes the `WFI` at a fixed cycle without being taken.
  The handler later clears the EVSYS flag and the NVIC pending bit itself
  (`vga.c:2225`).
- The wake rule is the ARMv6-M one: `WFI` wakes only for a pending interrupt
  that would preempt the current execution priority if PRIMASK were clear.
  A lower-priority pending interrupt (for example `TCC0` at priority 3) must
  not wake it. This needs real NVIC priorities.
- On blank lines the handler can return before the `TCC2` event, so the
  EVSYS interrupt is then really taken (`EVSYS_Handler`, `vga.c:983`).
- The handler has about 267 cycles before the wake point. If the emulator
  overestimates instruction costs, the handler reaches `WFI` late and the
  picture shifts. Slack before `WFI` is therefore a measured quantity.

**Pixels**

- Pixels are stores to PORT `OUT` through the single-cycle IOBUS alias.
  Bitmapped and 4bpp modes use `0x60000010` and alternate
  `STRH [port,#2]` and `STR [port]`; 8bpp uses `STRH` to `0x60000012`. Every
  mode outputs one pixel per 4 cycles; nothing is faster.
- Only `OUT[31:16]` carries colour. Pin map from the schematic netlist:
  PA16=R0, PA18=R1, PA17=R2, PA30=G0, PA22=G1, PA23=G2, PA19=B0, PA31=B1.
- The low half of a 32-bit store is junk. PA10 (buffer disable) and the sync
  pins are held by pin-mux functions, so that junk has no effect. The
  emulator ignores `OUT[15:0]` for video.
- 4bpp palette and colour remapping are plain RAM operations inside the
  handler; they need no emulator support.

**Other**

- Each line, visible or blank, the mixer writes one 10-bit sample to the
  DAC: 30 kHz mono.
- Milliseconds come from `TC4` `COUNT32` (continuous read request), counted
  from `TC3` at 1 kHz over EVSYS channel 1. Fairplay Race also uses SysTick.
- Boot: the loader sets SP from `[0x6000]` and jumps to `[0x6004]`. The
  game's own reset handler sets `SCB->VTOR` to `0x6000`.
- Games are built with `USE_BOOTLOADER`. USB and FAT calls are thunks:
  load the table pointer from `0x20B0`, index it, `BLX` (indices in
  `system.h`).
- The bootloader library calls back into the game. Redballs registers a
  keyboard-installed callback, stored in the game's RAM pointer table (the
  pointer at `0x20000000`, slot 3) and called on enumeration.
- None of the three shipped games call the FAT functions.
- `.usc` file: 512-byte header, preview of 96x72 bytes padded to 7168 bytes,
  binary at offset 7680. Header: `USVC` at 0, checksum at 4 (sum of
  little-endian words of the binary padded to 4 bytes), binary length at 8,
  short title at 32, four title lines at 64, four description lines at 192,
  two author lines at 320, date at 384, version at 416; each field 32 bytes.

## Architecture

Cargo workspace:

- `crates/usvc-core`: the portable core. No file I/O, threads, clock or
  platform dependencies. Everything else talks to it through this API:
  - `Machine::new(config)`, `load_usc(bytes)`, `load_bin(bytes, addr)`
  - `run_frame() -> FrameResult` (one frame is 525 `TCC1` overflows), plus
    `step()` and `run_cycles(n)`
  - `framebuffer() -> &[u32]`, 320x400 RGBA
  - `take_audio() -> Vec<i16>` at 30 kHz
  - `set_keyboard(state)`, `push_key(event)`, `set_gamepad(state)`
  - `trait Host` for file calls and debug text
  - inspection: registers, memory, fault info, cycle counters, trace hook
- `crates/usvc-cli`: headless runner. PNG screenshots, WAV audio, scripted
  input, traces, JSON run report. This is the tool Claude Code uses.
- `crates/usvc-sdl` (M6): window, live audio, keyboard and gamepad.
- `tools/` (Python, `uv`): `.usc` pack and unpack, symbol helpers, asset
  converters as needed.
- `sdk/`: kernel copy, linker script, startup code, build files, game
  template.
- `reference/`: upstream submodules, read-only.

Inside `usvc-core`:

- `cpu`: ARMv6-M Thumb interpreter with cycle counts (branch taken or not,
  load/store 2 cycles, 1 cycle on IOBUS, `LDM`/`STM`/`PUSH`/`POP` `1+n`,
  single-cycle multiply), PRIMASK, exception entry and return, VTOR, `WFI`.
  Flash wait states are not modelled; one fixed cost, documented.
- `nvic`: enable, pending, priorities, SysTick as a real counter.
- `bus`: flash, RAM, peripheral dispatch with byte, halfword and word
  access, IOBUS alias. Default for unknown registers: ready and lock status
  bits read 1, sync-busy bits read 0, everything else 0, logged once. A
  strict mode turns unknown access into an error.
- `sched`: next-event scheduler in CPU cycles; no per-cycle ticking.
- Peripherals:
  - modelled: PORT (`OUT` and its set, clear and toggle variants merged into
    one state; `DIR`, `PMUX`, `PINCFG`), TCC0/1/2, TC3/TC4 as a millisecond
    counter, EVSYS with its three channels, DAC.
  - accept-and-ignore: GCLK, PM, SYSCTRL, NVMCTRL, WDT, SERCOM0, and the
    bus-matrix and QoS registers (`0x41007000`, `0x41007110`, DMAC and USB
    `QOSCTRL`).
- `video`: records `(cycle, OUT[31:16])` per line relative to the `TCC2`
  wake point and rasterises on a 4-cycle grid. Fallback: nth write that
  covers bits 31:16 is the nth pixel.
- `hle`: synthetic bootloader. The table at `0x20B0` points to odd trap
  addresses inside `0x2000`-`0x5FFF`. Reaching one performs the call on the
  host, reading and writing guest structures as laid out in the kernel
  headers, then returns to `LR`. Traps can call back into guest code (set
  PC to the callback, LR to a return trap); the keyboard and gamepad
  "installed" callbacks fire on the first `usbHostTask`. `usbGetKey` keeps
  the kernel's key-queue behaviour.

## Milestones

M1-M4 need no ARM toolchain; the shipped `.usc` files are the test programs.

- **M0 Scaffold.** Workspace, `cargo test`, Python `.usc` unpacker, symbol
  table from the `.lss` listings.
- **M1 CPU.** Interpreter, bus, NVIC, stubs. Check: Tetris boots the way the
  loader starts it, sets VTOR, and returns from `initUsvc`; instruction
  unit tests pass.
- **M2 Timers and first picture.** Scheduler, TCC0/1/2, EVSYS, `WFI`, PORT
  capture, rasteriser, PNG. Checks the agent can run itself:
  `videoData.currentLineNumber` cycles 0-524, `currentFrame` advances once
  per frame, minimum slack before `WFI` is positive on every line, the
  handler ends before the next line. Then you approve one frame each of
  `Sprites8x8` (8bpp) and Tetris (bitmapped) against the photos in
  `reference/uSVC/images`, and those become golden hashes.
- **M3 All modes.** Redballs and the horse demo (4bpp, hi-res rows, palette
  remapping, fixed section, sprites) and Fairplay Race (8bpp, SysTick).
  Same pattern: you approve a frame, it becomes a golden hash.
- **M4 Input, sound, text.** HLE traps with guest callbacks, scripted input,
  DAC to WAV, emulator-only debug print for games (a write to a reserved
  address, harmless on hardware), JSON run report: faults with symbolised
  PC, stack high-water mark, per-frame cycles inside and outside the
  scanline interrupt, `WFI` slack. Check: a scripted Tetris session.
- **M5 SDK.** Install `arm-none-eabi-gcc`, replace the Atmel Studio build,
  `.usc` packer, game template, project `CLAUDE.md` describing the
  build-run-screenshot loop. Check: Tetris rebuilt from source produces the
  same golden frames as the shipped `Tetris.usc`. Starts alongside M1.
- **M6 Window.** SDL front end with live audio and real input.
- **Later.** Host-directory file access behind the `Host` trait, save
  states, wasm build, GDB stub, real USB host and SD emulation so the actual
  game loader runs.

## Testing

- Instruction unit tests, cross-checked with assembled programs once the
  toolchain is installed.
- Golden frame hashes for every shipped game at fixed frames with a fixed
  input script, frozen only after you approve the frame.
- Determinism: same binary and input always give the same frames and audio.

## Risks

- **Cycle budget.** Wrong instruction costs shift the picture rather than
  breaking it. The `WFI` slack check in M2 is the guard.
- **Toolchain.** Atmel Studio 7 used GCC 6.3.1 with
  `-O3 -mlong-calls -std=gnu99 -mcpu=cortex-m0plus`. Divided-syntax inline
  assembly is still the GCC default, so a current compiler will probably
  accept it. The likelier problem is `RAMFUNC`, a section-name string hack
  (`system.h:30`); the fix is the `.ramfunc` section the linker script
  already places. A newer compiler may also change the code around the
  timed assembly. Fallback: Arm's archived 6-2017-q2 release.
- **Hardware quirks the kernel works around** ("bus stalls"). The emulator
  will not reproduce them, so code that is wrong on hardware for that
  reason can still look right here.
- **HLE coverage.** A game built with its own USB stack gets no input until
  the USB controller is emulated. All shipped games use the bootloader
  table.
- **No written hardware spec** beyond the schematic and source comments; the
  SAMD21 datasheet is the reference for peripheral behaviour.

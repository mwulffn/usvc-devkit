# How the uSVC console and its kernel work

Notes taken while writing the emulator. They come from reading the kernel
source in `reference/uSVC/software/uSVC_Template_Project/usvc_kernel` (line
numbers below refer to that copy) and the schematic, and from measurements in
the emulator. Nothing here has been checked against a real console.

For the console itself, see [next-hack's repository](https://github.com/next-hack/uSVC).

## The machine

- Microchip ATSAMD21E18: Cortex-M0+ at 48 MHz, 256 KB flash, 32 KB RAM at
  `0x20000000`.
- VGA output of 320x200 pixels with 256 colours. Each pixel row is sent
  twice, so the monitor sees 400 lines.
- One 10-bit DAC for sound, a USB host port for keyboard or gamepad, and a
  micro-SD card.
- No video chip. The CPU writes every pixel to an I/O port at the right
  moment, and that takes about 80% of its time.

## Memory layout

| Address | Contents |
|---|---|
| `0x0000` | Left free by the linker scripts, for uChip's own bootloader |
| `0x2000` | uSVC game loader (menu, SD card, USB library) |
| `0x6000` | The game. The loader sets the stack pointer from `[0x6000]` and jumps to `[0x6004]`; the game's reset handler then sets the vector table base to `0x6000`. |
| `0x20000000` | One pointer, to the game's table of RAM pointers that the loader's library uses |
| `0x20000004` | Game RAM |

## Frame and line timing

- A line is 1600 CPU cycles (30 kHz). A frame is 525 lines (about 57 Hz).
- Lines 0-399 are drawn. Lines 400-524 are the vertical blank, which is when
  a game gets most of its CPU time.
- Three timers do the synchronisation, with no software in the loop:
  - `TCC1` runs with period 1600 and drives the horizontal sync pin. Its
    overflow increments `TCC0` through the event system.
  - `TCC0` counts lines (period 525) and drives the vertical sync pin.
  - `TCC2` runs with the same period as `TCC1`. Its compare match at count
    268 raises the event-system interrupt used for pixel alignment (below).
    The kernel writes `TCC2`'s event control register twice in a row
    (`vga.c:961-963`); the second write replaces the first, so as far as we
    can tell `TCC2` is never retriggered and simply free-runs a few cycles
    behind `TCC1`.

## How a line is drawn without jitter

Interrupt latency on a Cortex-M0+ varies by a few cycles, which would make
the picture wobble. The kernel removes that this way:

1. `TCC1` compare 1 raises the scanline interrupt (priority 1) one cycle
   into the line. `TCC1_Handler` runs from RAM.
2. The handler masks interrupts, runs the audio mixer, and prepares the
   line's data.
3. It executes `WFI` with interrupts still masked.
4. The `TCC2` compare match sets the event-system interrupt pending
   (priority 0). On ARMv6-M a pending interrupt that would preempt wakes
   `WFI` even when it is masked. The CPU continues at an exact cycle,
   without taking the interrupt.
5. The handler writes the pixels, then clears the event flag and the pending
   bit itself.

On blank lines the handler returns early, so the event-system interrupt is
taken for real; its handler only clears the flag.

The handler has about 270 cycles between entry and the wake point. If it
arrives late, `WFI` falls straight through and the line is shifted. The
emulator reports the smallest margin as `min_wfi_slack`. Measured margins, in
cycles: Tetris 87, the sprite demos 126, Fairplay Race 44, the horse demo 16,
Redballs 15, Shardbreaker 48.

## How pixels reach the monitor

- Eight pins of port A carry the colour through resistors: PA16 = R0,
  PA18 = R1, PA17 = R2, PA30 = G0, PA22 = G1, PA23 = G2, PA19 = B0,
  PA31 = B1 (from the schematic netlist). That is 3 bits red, 3 green and 2
  blue, in a scrambled bit order.
- A pixel byte in the 8bpp mode uses that order: bit 0 R0, bit 1 R2,
  bit 2 R1, bit 3 B0, bit 4 G0, bit 5 B1, bit 6 G1, bit 7 G2. This is the
  kernel's `COLOR_TORGB332` macro and what `tools/gfx.py` produces.
- Pixels are stores to the port's `OUT` register through the single-cycle
  I/O bus alias at `0x60000010`. Every video mode outputs one pixel every 4
  cycles.
- Only the upper 16 bits of `OUT` matter for the picture. Modes that store
  32 bits put junk in the lower half; the pins there are held by other
  peripheral functions so it has no effect.

Measured in the emulator: the first visible line starts 76 lines after
vertical sync. The first pixel is written 274 cycles into the line in
bitmapped mode, 277 in 4bpp tile mode and 279 in 8bpp tile mode.

## Video modes

A game picks one at compile time with `GFX_MODE` in `usvc_config.h`.

| Mode | What it is | Used by |
|---|---|---|
| `BITMAPPED_MODE` | 320x200 at 2 bits per pixel. A palette entry describes two neighbouring pixels; optional per-8x1-block palettes. | Tetris, `games/hello` |
| `TILE_MODE1` | 8x8 tiles at 8 bits per pixel (64 bytes each), sprites, scrolling. | Fairplay Race, the sprite demos, Shardbreaker |
| `TILE_MODE2` | 8x8 tiles at 4 bits per pixel (32 bytes each), palettes that can change per line. | Redballs, the horse demo |

In the tile modes:

- `vram` holds one 16-bit entry per tile position: the low half of the
  tile's RAM address. Tiles shown on screen must therefore be in RAM.
- Sprites are drawn by copying each tile they overlap into a spare RAM tile,
  drawing the sprite into the copy, and pointing `vram` at it.
  `restoreBackgroundTiles()` undoes that.
- Sprite pixel value 0 is transparent.

## Sound

- The mixer runs at the top of the scanline interrupt on every line, visible
  or blank, and writes one sample to the DAC: 30 kHz mono, four channels.
- The sound engine above it is a port of the Uzebox engine: patches (lists
  of timed commands) for instruments and effects, and a MIDI-like stream for
  songs. `soundEngine()` must be called once per frame.
- The mixer centres its output at a quarter of the DAC range.

## Time

`millis()` reads a 32-bit timer (`TC4`) that counts 1 kHz events from `TC3`.
No interrupt is involved.

## The bootloader library

Games are built with `USE_BOOTLOADER`, which leaves the USB host stack and
the FAT file system out of the game. Calls to them go through a table of
function pointers owned by the game loader: the table's address is stored at
`0x2000 + 44*4`, and the indices are the enumeration in `system.h`.

The library calls back into the game. For example, Redballs registers a
function to run when a keyboard has been set up.

## The `.usc` package

| Offset | Size | Contents |
|---|---|---|
| 0 | 4 | `USVC` |
| 4 | 4 | Checksum: sum of the binary's little-endian 32-bit words |
| 8 | 4 | Length of the binary, padded to 4 bytes |
| 32 | 32 | Short title |
| 64 | 4 x 32 | Title lines |
| 192 | 4 x 32 | Description lines |
| 320 | 2 x 32 | Author lines |
| 384 | 32 | Date |
| 416 | 32 | Version |
| 512 | 7168 | Preview, 96x72 bytes in the 8bpp pixel format, padded to whole 512-byte sectors |
| 7680 | | The binary, as linked at `0x6000` |

The game loader shows the text fields in a column 15 characters wide.
`tools/usc.py` reads and writes this format and reproduces all six upstream
packages byte for byte.

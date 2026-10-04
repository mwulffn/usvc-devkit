# Making a game

How to write a uSVC game with this repository: from an empty directory to a
package, tested in the emulator. `games/hello` is the smallest working game
and `games/shardbreaker` is a complete one; this guide points into both.

The kernel that games are built on is next-hack's and lives under
`reference/`. Its headers are the API reference; this guide covers what is
not obvious from them. [hardware.md](hardware.md) explains why the rules
below exist.

## Setting up

You need Rust, SDL2, `uv` and the Arm GNU toolchain (see the top-level
README), and the submodules:

```sh
git submodule update --init --depth 1
cargo build --release
```

## The smallest game

Copy `games/hello` to `games/yourgame`. It has three files:

- `usvc_config.h`: which video mode, whether there are sprites and sound,
  and how much memory each gets. The kernel is compiled against this file.
- `main.h`: includes the kernel.
- `main.c`: the game.

Build and run it:

```sh
make -C sdk GAME=../games/yourgame EMULATOR=1 run     # headless; writes a PNG
make -C sdk GAME=../games/yourgame EMULATOR=1 play    # in a window
make -C sdk GAME=../games/yourgame                    # the package for a console
```

Output goes to `build/yourgame-emu/` with `EMULATOR=1` and `build/yourgame/`
without. The package is `yourgame.usc`; the `.lss` file beside it is a
listing with symbol names.

Useful make variables: `FRAMES=300` (how long `run` runs), `RUNFLAGS="..."`
(extra `usvc` options, for example `--tap 60:SPACE`), `NAME=`, `TITLE=`.
A game can set its own in a `game.mk` file.

The four linker warnings ending in "is not implemented and will always fail"
are expected.

## The frame loop

Every game has this shape (`games/shardbreaker/main.c`):

```c
initUsvc(patches);              /* or NULL without sound */
while (1)
{
	waitForVerticalBlank();     /* the picture has just been drawn */
	soundEngine();              /* once per frame, if sound is enabled */
	/* ... update the game and the screen ... */
	do
	{
		usbHostTask();          /* keep USB alive and read input */
		/* ... poll keyboard and gamepad ... */
	} while (getCurrentScanLineNumber() < 523);
}
```

During the 400 visible lines the kernel uses nearly all of the CPU to draw.
Your code gets the vertical blank: about 125 lines, or 200,000 CPU cycles,
per frame. Everything that changes the picture must happen there.

## Rules that are easy to break

1. **Do the frame's work right after `waitForVerticalBlank()` and finish
   before line 524.** If it runs over, the top of the next picture is drawn
   from half-updated data. Shardbreaker's emulator build prints the line at
   which each frame's work ended (search `main.c` for "frame work").
2. **In tile modes, a tile on screen must be in RAM.** `vram` stores only
   the low half of a tile's address. Copy tile data into the kernel's
   `tiles[]` array at start-up and tell it how many you use with
   `setNumberOfRamTiles()`. An 8bpp tile is 64 bytes.
3. **Leave spare tiles for sprites.** Every tile a sprite overlaps uses one
   more RAM tile while it is drawn. `MAX_TILES` must cover your own tiles
   plus `MAX_TEMP_SPRITE_TILES`.
4. **Change `vram` only between `restoreBackgroundTiles()` and
   `drawSprites()`.** While sprites are drawn, some `vram` entries point at
   the temporary tiles; writing to them then is lost or leaves debris.
5. **Sprite pixel value 0 is transparent.** A sprite cannot contain pure
   black; `tools/gfx.py` stores black as the darkest red.
6. **The kernel looks for some names in your game.** With sprites:
   `frameData` (and the pixel data it points to). With sound: `soundWaves`,
   `getNumberOfSoundWaves()` and `getNumberOfPatches()`, plus the patch
   array you pass to `initUsvc()`. Their headers must be included at the end
   of `usvc_config.h`.
7. **32 KB of RAM is all there is.** `vram`, the RAM tiles, the kernel's
   state and your variables share it with a 1,152-byte stack. Keep tables
   `const` so they stay in flash. `arm-none-eabi-size` output is printed
   after each build: `data + bss` is RAM use.
8. **Never edit files under `reference/`.** A game that needs a changed
   kernel can carry its own copy in a `usvc_kernel/` directory, which the
   Makefile then uses.

## Drawing

Bitmapped mode (`games/hello`): write pixels into `pixels[]` or use
`printText()`. A palette entry describes two neighbouring pixels at once;
see `initPalette()` in `games/hello/main.c`.

8bpp tile mode (`games/shardbreaker/screen.c`):

- `placeTile(column, row, n)` shows RAM tile `n` at a position.
- Text is tiles too. Shardbreaker builds a font into RAM tiles at start-up
  with the kernel's `putCharInTile()`.
- Each frame: `restoreBackgroundTiles()`, change tiles, call
  `putSprite(slot, x, y, flags, frame)` for each sprite to show and
  `removeSprite(slot)` for each to hide, then `drawSprites()`. Higher slot
  numbers are drawn on top.

Colours are 3 bits red, 3 bits green and 2 bits blue. Blue has only four
levels, which limits dark and pastel shades.

## Artwork

Artwork is drawn as PNG files and converted to C:

```sh
make -C sdk GAME=../games/yourgame assets
```

This runs `tools/gfx.py` on the files in the game's `assets/` directory and
writes C files into `gen/`, which the build picks up. Commit `gen/`.

- `assets/tiles.png` with `assets/tiles.txt`: the image is cut into 8x8
  tiles in reading order; the text file gives each tile a name, one per
  line. You get `tileData[]` and a `TILE_<NAME>` index for each.
- `assets/sprites.png` with `assets/frames.txt`: each line names a rectangle
  (`name x y width height [handle_x handle_y]`). You get `spriteData[]`,
  `frameData[]` and a `FRAME_<NAME>` index for each. Transparent pixels in
  the PNG become transparent in the sprite.
- `assets/preview.png`: any image; it becomes the 96x72 picture shown in the
  console's game menu.

Colours are rounded to the nearest the console can show, so draw with those
colours to avoid surprises.

## Sound and music

Sound effects are patches: short lists of timed commands in C. See
`games/shardbreaker/patches.c`. Play one with `triggerFx()`.

Songs are text files in the game's `music/` directory, converted by
`tools/song.py` as part of the `assets` target:

```text
tempo 6          # video frames per step
patch 0 10       # channel 0 plays patches[10]
loop             # everything below repeats

0: a4 -  c5 -  e5 .  .  .
1: a2 -  -  -  a2 -  -  -
```

Each row is one channel; a step is a note, `-` to let it ring, or `.` to
stop it. Channels 0-2 are for music; the kernel uses channel 3 first for
effects. Play a song with `startSong(titleSong)`. The header of `song.py`
describes the format in full.

## Input

`readKeyboard()` and `readGamepad()` in `games/shardbreaker/main.c` show
both devices:

- Keyboard: `usbGetCurrentAsciiKeyboardStateEx()` fills six entries with the
  keys held now, as ASCII or, for keys without a character, the USB key code
  in the high byte.
- Gamepad: `getCurrentGamepadState()` gives buttons as a bit mask and axes
  with their minimum and maximum. Axes are analogue; the upstream games only
  test for the extremes.

Check `usbHidBootKeyboardIsInstalled()` and
`usbHidGenericGamepadIsInstalled()` before reading.

## The package

The console's game menu shows a title, a description, an author, a date, a
version and a picture. Set the text in the game's `game.mk` (see
Shardbreaker's); each line is shown at most 15 characters wide. The picture
comes from `assets/preview.png`.

`tools/usc.py info FILE.usc` prints what a package contains.

## Testing in the emulator

```sh
# run 300 frames, press space at frame 60, hold D for 40 frames from 130
./target/release/usvc build/yourgame-emu/yourgame.usc --headless --frames 300 \
    --tap 60:SPACE --tap 130:D:40 --png out/shot.png \
    --lss build/yourgame-emu/yourgame.lss
```

Then look at the PNG and the summary. In the summary, `lines_drawn` should
be 400 per frame, `min_wfi_slack` above zero, and there should be no `FAULT`
line. [emulator.md](emulator.md) explains every field and option.

Text output: include `usvc_debug.h` and call `usvcDebugPrint("...")`. It
prints on the emulator's standard output in `EMULATOR=1` builds and compiles
to nothing otherwise. Do not write to the debug port any other way: the
address faults on a real console.

Aids worth copying from Shardbreaker:

- A self-playing demo, which doubles as a long-running test.
- Keys that exist only in emulator builds (`#ifdef USVC_EMULATOR`) to hand
  out power-ups, clear a level, or switch on an autopilot.
- The end-of-frame line report mentioned above.

## Before calling it done

- Build without `EMULATOR=1` and run that package too. It is the one that
  goes on the SD card.
- The emulator is more forgiving than a console: it has no flash wait states
  and does not model every bus effect. Leave a margin in frame time.
- Nothing in this repository has yet run on real hardware. If you try it on
  a console, please report what you see.

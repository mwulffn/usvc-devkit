/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright (C) 2026 Michael Wulff Nielsen */
/* Kernel configuration for Shardbreaker: 8bpp tiles with sprites and sound. */
#ifndef USVC_CONFIG_H_
#define USVC_CONFIG_H_
#include <stdint.h>

#define GFX_MODE TILE_MODE1
#define VRAMX 42
#define VRAMY 26

/* Tiles kept in RAM: the artwork tiles followed by the font. The sprite
   engine needs spare tiles after those for wherever a sprite overlaps. */
#define GAME_RAM_TILES 104
#define MAX_TEMP_SPRITE_TILES 64
#define MAX_TILES (GAME_RAM_TILES + MAX_TEMP_SPRITE_TILES)

#define SPRITES_ENABLED 1
#define ENABLE_TRANSPARENT_SPRITES 0
#define MAX_ONSCREEN_SPRITES 12
#define ENABLE_SPRITE_PRIORITY 0
#define ENABLE_TILE_PRIORITY 0
#define TILES_ALL_OPAQUE 1

#define MAX_NUMBER_OF_PALETTES 2
#define ENABLE_PALETTE_REMAPPING 0
#define ENABLE_PER_LINE_COLOR_REMAPPING 0
#define ENABLE_HIRES_PER_LINE_COLOR_REMAPPING 0
#define ENABLE_PALETTE_ROW_REMAPPING 0
#define USE_ROW_REMAPPING 0
#define USE_HIRES_ROW_REMAPPING 1
#define PER_LINE_X_SCROLL 0
#define PER_TILE_X_SCROLL 0

#define USE_SECTION NO_FIXED_SECTION
#define SECTION_LIMIT 2
#define USE_SEPARATE_FIXED_SECTION_PALETTE 0
#define FIXED_SECTION_PALETTE_INDEX 0
#define FIXED_SECTION_MAPSIZEX 40
#define FIXED_SECTION_MAPSIZEY 3
#define MAX_FIXED_SECTION_TILES 16

#define AUDIO_ENABLED 1
#define USE_MIXER 1
#define INCLUDE_DEFAULT_WAVES 1
#define AUDIO_USES_LPF 0
#define MUSIC_ENGINE MIDI

#define USE_USB_HUB 0
#define USB_NUMDEVICES 1
#define NUMBER_OF_USB_PIPES 3
#define MAX_USB_INTERFACES 4

/* Without the game loader, the USB and FAT code must be linked in. */
#ifndef USE_BOOTLOADER
	#define FORCE_INCLUDE_USB_GAMEPAD_MODULE 1
	#define FORCE_INCLUDE_USB_MODULE 1
	#define FORCE_INCLUDE_USB_KEYBOARD_MODULE 1
#endif

/* Data the kernel refers to by name. */
#include "gen/sprites.h"
#include "soundWaveList.h"
#include "patches.h"

#endif /* USVC_CONFIG_H_ */

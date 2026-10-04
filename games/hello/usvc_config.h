/* Kernel configuration for the hello template: bitmapped mode, no audio. */
#ifndef USVC_CONFIG_H_
#define USVC_CONFIG_H_
#include <stdint.h>

/* BITMAPPED_MODE: 320x200, 2 bits per pixel. TILE_MODE1: 8bpp tiles.
   TILE_MODE2: 4bpp tiles. See the upstream template for all tile options. */
#define GFX_MODE BITMAPPED_MODE
#define PER_HORIZONTAL_BLOCK_PALETTE_REMAP 0

#define SPRITES_ENABLED 0
#define ENABLE_TILE_PRIORITY 0
#define ENABLE_SPRITE_PRIORITY 0

#define AUDIO_ENABLED 0
#define USE_MIXER (AUDIO_ENABLED)
#define INCLUDE_DEFAULT_WAVES 1

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

#endif /* USVC_CONFIG_H_ */

#ifndef MAIN_H_
#define MAIN_H_
#include "usvc_kernel/usvc_kernel.h"
#include "usvc_debug.h"
#include "gen/tiles.h"
#include "gen/sprites.h"
#include "gen/songs.h"
/* Defined in the kernel's vga.c but missing from its headers. */
void removeSprite(uint16_t num);

#include "screen.h"
#include "levels.h"
#include "game.h"
#endif /* MAIN_H_ */

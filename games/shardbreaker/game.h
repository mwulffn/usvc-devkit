/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright (C) 2026 Michael Wulff Nielsen */
#ifndef GAME_H_
#define GAME_H_
#include <stdint.h>

#define INPUT_MOVE_MAX 127
#define CHEAT_NEXT_LEVEL 9
#define CHEAT_AUTOPILOT 10

/* What the player is asking for this frame, whatever the device. */
typedef struct
{
	int8_t move;		/* paddle speed, -INPUT_MOVE_MAX (left) to INPUT_MOVE_MAX */
	uint8_t fire;		/* launch the ball, confirm */
	uint8_t pause;		/* pause or continue the game */
	uint8_t cheat;		/* emulator builds only: 1-6 capsule, or a CHEAT_ value; else 0 */
} input_t;

void gameInit(void);
/* Advance the game by one frame and queue the sprites to draw. */
void gameUpdate(const input_t *input);
#endif /* GAME_H_ */

/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright (C) 2026 Michael Wulff Nielsen */
#ifndef LEVELS_H_
#define LEVELS_H_
#include <stdint.h>

#define BRICK_COLUMNS 13
#define BRICK_ROWS 14

/* Brick kinds as stored in the playfield. 1-8 are the one-hit colours. */
#define BRICK_NONE 0
#define BRICK_COLOURS 8
#define BRICK_STEEL 9			/* takes two hits */
#define BRICK_STEEL_CRACKED 10
#define BRICK_GOLD 11			/* cannot be broken */

#define NUM_LEVELS 6

/* Fill `bricks` with level `n` (0-based); returns how many can be broken. */
int loadLevel(int n, uint8_t bricks[BRICK_ROWS][BRICK_COLUMNS]);
#endif /* LEVELS_H_ */

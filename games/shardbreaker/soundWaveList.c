/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright (C) 2026 Michael Wulff Nielsen */
/* The sound waves the audio engine can play: only the kernel's built-in ones. */
#include "main.h"

#define DEFAULT_WAVE(data) { .length = 256, .wData = (int8_t *) data, .sps = 30000 }

const soundWave_t soundWaves[] =
{
	DEFAULT_WAVE(sineWave),
	DEFAULT_WAVE(sawToothWave),
	DEFAULT_WAVE(triangleWave),
	DEFAULT_WAVE(squareWave25),
	DEFAULT_WAVE(squareWave50),
	DEFAULT_WAVE(squareWave75),
	DEFAULT_WAVE(sineDistoWave1),
	DEFAULT_WAVE(sineDistoWave2),
	DEFAULT_WAVE(sineDistoWave3),
	DEFAULT_WAVE(squareWave50Filtered),
};

int getNumberOfSoundWaves()
{
	return sizeof(soundWaves) / sizeof(soundWave_t);
}

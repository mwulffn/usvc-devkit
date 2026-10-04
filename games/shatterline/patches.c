/*
 * Sound effects. Each patch is a list of (frames to wait, command, value);
 * PC_PITCH takes a MIDI note number.
 */
#include "main.h"

static const char wallPatch[] =
{
	0, PC_WAVE, WAVE_TRIANGLE,
	0, PC_PITCH, 62,
	0, PC_ENV_SPEED, -24,
	3, PC_NOTE_CUT, 0,
	0, PATCH_END
};
static const char paddlePatch[] =
{
	0, PC_WAVE, WAVE_SQUARE_50_FILTERED,
	0, PC_PITCH, 57,
	0, PC_ENV_SPEED, -16,
	2, PC_NOTE_UP, 12,
	3, PC_NOTE_CUT, 0,
	0, PATCH_END
};
static const char brickPatch[] =
{
	0, PC_WAVE, WAVE_SQUARE_25,
	0, PC_PITCH, 81,
	0, PC_ENV_SPEED, -20,
	1, PC_NOTE_UP, 7,
	1, PC_NOTE_UP, 5,
	3, PC_NOTE_CUT, 0,
	0, PATCH_END
};
static const char steelPatch[] =
{
	0, PC_WAVE, WAVE_DISTORTED_2,
	0, PC_PITCH, 93,
	0, PC_ENV_SPEED, -30,
	2, PC_NOTE_DOWN, 1,
	3, PC_NOTE_CUT, 0,
	0, PATCH_END
};
static const char capsulePatch[] =
{
	0, PC_WAVE, WAVE_SINE,
	0, PC_PITCH, 72,
	0, PC_ENV_SPEED, -6,
	3, PC_NOTE_UP, 4,
	3, PC_NOTE_UP, 3,
	3, PC_NOTE_UP, 5,
	6, PC_NOTE_CUT, 0,
	0, PATCH_END
};
static const char losePatch[] =
{
	0, PC_WAVE, WAVE_SAWTOOTH,
	0, PC_PITCH, 60,
	0, PC_ENV_SPEED, -4,
	4, PC_NOTE_DOWN, 3,
	4, PC_NOTE_DOWN, 3,
	4, PC_NOTE_DOWN, 4,
	4, PC_NOTE_DOWN, 5,
	12, PC_NOTE_CUT, 0,
	0, PATCH_END
};
static const char clearPatch[] =
{
	0, PC_WAVE, WAVE_SQUARE_50_FILTERED,
	0, PC_PITCH, 72,
	0, PC_ENV_SPEED, -3,
	5, PC_NOTE_UP, 4,
	5, PC_NOTE_UP, 3,
	5, PC_NOTE_UP, 5,
	5, PC_NOTE_UP, 4,
	5, PC_NOTE_UP, 3,
	16, PC_NOTE_CUT, 0,
	0, PATCH_END
};
static const char launchPatch[] =
{
	0, PC_WAVE, WAVE_SINE,
	0, PC_PITCH, 64,
	0, PC_ENV_SPEED, -12,
	2, PC_NOTE_UP, 12,
	4, PC_NOTE_CUT, 0,
	0, PATCH_END
};

static const char laserPatch[] =
{
	0, PC_WAVE, WAVE_SAWTOOTH,
	0, PC_PITCH, 96,
	0, PC_ENV_SPEED, -28,
	1, PC_NOTE_DOWN, 6,
	1, PC_NOTE_DOWN, 6,
	1, PC_NOTE_DOWN, 6,
	2, PC_NOTE_CUT, 0,
	0, PATCH_END
};
static const char enemyPatch[] =
{
	0, PC_WAVE, WAVE_DISTORTED_1,
	0, PC_PITCH, 76,
	0, PC_ENV_SPEED, -10,
	2, PC_NOTE_DOWN, 5,
	2, PC_NOTE_UP, 9,
	2, PC_NOTE_DOWN, 12,
	4, PC_NOTE_CUT, 0,
	0, PATCH_END
};
/* Instruments: how a note sounds over time. */
static const char leadPatch[] =
{
	0, PC_ENV_SPEED, -6,
	0, PATCH_END
};
static const char bassPatch[] =
{
	0, PC_ENV_VOL, 255,
	0, PC_ENV_SPEED, -9,
	0, PATCH_END
};
static const char arpeggioPatch[] =
{
	0, PC_ENV_SPEED, -30,
	0, PATCH_END
};

#define FX_PATCH(stream) { 0, 0, (uint8_t *) stream, 0, 256 }
#define INSTRUMENT(wave, stream) { 0, wave, (uint8_t *) stream, 0, 256 }

/* Same order as the enumeration in patches.h. */
const patch_t patches[] =
{
	FX_PATCH(wallPatch),
	FX_PATCH(paddlePatch),
	FX_PATCH(brickPatch),
	FX_PATCH(steelPatch),
	FX_PATCH(capsulePatch),
	FX_PATCH(losePatch),
	FX_PATCH(clearPatch),
	FX_PATCH(launchPatch),
	FX_PATCH(laserPatch),
	FX_PATCH(enemyPatch),
	INSTRUMENT(WAVE_SQUARE_50_FILTERED, leadPatch),
	INSTRUMENT(WAVE_TRIANGLE, bassPatch),
	INSTRUMENT(WAVE_SQUARE_25, arpeggioPatch),
};

int getNumberOfPatches()
{
	return sizeof(patches) / sizeof(patch_t);
}

void playFx(int fx)
{
	triggerFx(fx, 255, FX_FLAGS_RETRIG, 0x10000);	/* 0x10000: no detuning */
}

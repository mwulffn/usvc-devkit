#ifndef SOUNDWAVELIST_H_
#define SOUNDWAVELIST_H_
#include "usvc_kernel/audio.h"

/* Indices into soundWaves[], for PC_WAVE in patches. */
enum
{
	WAVE_SINE,
	WAVE_SAWTOOTH,
	WAVE_TRIANGLE,
	WAVE_SQUARE_25,
	WAVE_SQUARE_50,
	WAVE_SQUARE_75,
	WAVE_DISTORTED_1,
	WAVE_DISTORTED_2,
	WAVE_DISTORTED_3,
	WAVE_SQUARE_50_FILTERED
};

extern const soundWave_t soundWaves[];
int getNumberOfSoundWaves();
#endif /* SOUNDWAVELIST_H_ */

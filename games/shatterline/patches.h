#ifndef PATCHES_H
#define PATCHES_H
#include "usvc_kernel/audio.h"

/* Sound effects and instruments, as indices into patches[]. */
enum
{
	FX_WALL,
	FX_PADDLE,
	FX_BRICK,
	FX_STEEL,
	FX_CAPSULE,
	FX_LOSE,
	FX_CLEAR,
	FX_LAUNCH,
	FX_LASER,
	FX_ENEMY,
	/* Instruments for the songs; the .song files refer to these numbers. */
	PATCH_LEAD,		/* 10 */
	PATCH_BASS,		/* 11 */
	PATCH_ARPEGGIO	/* 12 */
};

extern const patch_t patches[];
int getNumberOfPatches();
void playFx(int fx);
#endif

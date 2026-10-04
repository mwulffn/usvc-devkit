#ifndef GAME_H_
#define GAME_H_
#include <stdint.h>

#define INPUT_MOVE_MAX 127

/* What the player is asking for this frame, whatever the device. */
typedef struct
{
	int8_t move;		/* paddle speed, -INPUT_MOVE_MAX (left) to INPUT_MOVE_MAX */
	uint8_t fire;		/* launch the ball, confirm */
	uint8_t cheat;		/* emulator builds only: capsule 1-6 to hand out, or 0 */
} input_t;

void gameInit(void);
/* Advance the game by one frame and queue the sprites to draw. */
void gameUpdate(const input_t *input);
#endif /* GAME_H_ */

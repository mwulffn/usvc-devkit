/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright (C) 2026 Michael Wulff Nielsen */
/*
 * Shardbreaker game logic: paddle, balls, bricks, capsules, laser, wisps,
 * game states and the self-playing demo.
 */
#include "main.h"

/* Positions and speeds of balls are in 1/256 pixel. */
#define FP 8
#define ONE (1 << FP)

#define BALL_SIZE 6
#define PADDLE_Y 184
#define PADDLE_HEIGHT 8
#define PADDLE_WIDTH 32
#define PADDLE_WIDTH_WIDE 48
#define PADDLE_MAX_SPEED 4		/* pixels per frame at full deflection */
/* How far below the paddle top a ball may be and still be returned. */
#define PADDLE_CATCH_DEPTH 6
/* A caught ball is let go by itself after this long. */
#define CATCH_HOLD_FRAMES 170

#define MAX_BALLS 3
#define MAX_CAPSULES 2
#define CAPSULE_WIDTH 16
#define CAPSULE_HEIGHT 8
#define CAPSULE_FALL_SPEED 1
#define CAPSULE_ODDS 6			/* one brick in this many drops a capsule */

#define MAX_SHOTS 4				/* two pairs of laser bolts */
#define SHOT_WIDTH 2
#define SHOT_HEIGHT 6
#define SHOT_SPEED 4
#define SHOT_COOLDOWN_FRAMES 14
#define SHOT_INSET 4			/* distance of the guns from the paddle ends */

#define MAX_WISPS 2
#define WISP_SIZE 12
#define WISP_SPAWN_FRAMES 420
#define WISP_ANIMATION_FRAMES 16
#define WISP_TURN_ODDS 48		/* changes direction about once in this many frames */

/* Ball speed in 1/256 pixel per frame. It grows with paddle hits. */
#define SPEED_START (2 * ONE)
#define SPEED_MAX (7 * ONE / 2)
#define SPEED_STEP (ONE / 8)
/* Each level starts this much faster than the one before. */
#define LEVEL_SPEED_STEP (ONE / 16)
#define HITS_PER_SPEED_STEP 6
/* The ball moves in this many steps per frame so it cannot skip a brick. */
#define MOVE_STEPS 2

#define START_LIVES 3
#define MAX_LIVES 5
#define MESSAGE_ROW 19
#define LOST_PAUSE_FRAMES 60
#define CLEAR_PAUSE_FRAMES 100
/* Clearing the last level of a round earns a bonus and a longer celebration. */
#define ALL_CLEAR_PAUSE_FRAMES 280
#define LEVEL_BONUS 1000
#define ALL_CLEAR_BONUS 10000
/* The title screen starts a self-playing demo after this long. */
#define DEMO_START_FRAMES 900
#define DEMO_SERVE_FRAMES 40
#define TITLE_PROMPT_ROW 17
#define TITLE_BLINK_FRAMES 32

/* Sprite slots; higher numbers are drawn on top. */
#define SPRITE_PADDLE 0
#define SPRITE_CAPSULE 1
#define SPRITE_SHOT (SPRITE_CAPSULE + MAX_CAPSULES)
#define SPRITE_WISP (SPRITE_SHOT + MAX_SHOTS)
#define SPRITE_BALL (SPRITE_WISP + MAX_WISPS)
_Static_assert(SPRITE_BALL + MAX_BALLS <= MAX_ONSCREEN_SPRITES,
	"raise MAX_ONSCREEN_SPRITES in usvc_config.h");

enum { STATE_TITLE, STATE_SERVE, STATE_PLAY, STATE_LOST, STATE_CLEAR, STATE_GAME_OVER };
/* Capsule types, in the order of their sprite frames. */
enum
{
	CAPSULE_EXPAND, CAPSULE_SLOW, CAPSULE_MULTI, CAPSULE_LIFE, CAPSULE_LASER, CAPSULE_CATCH,
	CAPSULE_TYPES
};
/* Paddle modes, in the order of the paddle sprite frames. Only one at a time. */
enum { PADDLE_NORMAL, PADDLE_WIDE, PADDLE_LASER, PADDLE_CATCH };

typedef struct
{
	int32_t x, y;		/* top-left corner, fixed point */
	int16_t vx, vy;
	uint8_t active;
	uint8_t held;		/* stuck to the paddle (catch) */
	int16_t holdOffset;	/* x relative to the paddle while held */
	uint8_t holdTimer;
} ball_t;

/* Capsules, laser bolts and wisps: things with a pixel position. */
typedef struct
{
	int16_t x, y;
	int8_t vx, vy;
	uint8_t type;
	uint8_t active;
} object_t;

static struct
{
	uint8_t state;
	uint16_t timer;
	uint16_t frame;				/* counts every frame, for animation */
	uint8_t bricks[BRICK_ROWS][BRICK_COLUMNS];
	int remaining;				/* breakable bricks left */
	int level;
	int lives;
	uint32_t score;
	uint32_t highScore;
	int paddleX;
	uint8_t paddleMode;
	int16_t speed;
	uint8_t paddleHits;
	uint8_t lastFire;
	uint8_t lastPause;
	uint8_t paused;
	uint8_t lastCheat;
	uint8_t autopilot;			/* testing aid: the paddle plays by itself */
	uint8_t shotCooldown;
	uint16_t wispTimer;
	uint8_t demo;				/* the game is playing itself */
	uint8_t demoAim;			/* where on the paddle the demo meets the ball */
	ball_t balls[MAX_BALLS];
	object_t capsules[MAX_CAPSULES];
	object_t shots[MAX_SHOTS];
	object_t wisps[MAX_WISPS];
} g;

/* Bounce directions off the paddle: sine and cosine (x256) of 0 to 60 degrees
   from vertical, in 10 degree steps. */
#define BOUNCE_ANGLES 7
static const uint8_t bounceSin[BOUNCE_ANGLES] = {0, 44, 88, 128, 165, 196, 222};
static const uint16_t bounceCos[BOUNCE_ANGLES] = {256, 252, 241, 222, 196, 165, 128};

static int paddleWidth(void)
{
	return g.paddleMode == PADDLE_WIDE ? PADDLE_WIDTH_WIDE : PADDLE_WIDTH;
}

/* The demo is silent, like an arcade machine waiting for a coin. */
static void fx(int effect)
{
	if (!g.demo)
		playFx(effect);
}

static void playMusic(const uint8_t *song)
{
	if (!g.demo)
		startSong(song);
}

static int overlaps(int ax, int ay, int aw, int ah, int bx, int by, int bw, int bh)
{
	return ax < bx + bw && bx < ax + aw && ay < by + bh && by < ay + ah;
}

/* Send the ball upwards at `angle` steps from vertical (negative: left). */
static void aimBall(ball_t *ball, int angle)
{
	int index = angle < 0 ? -angle : angle;
	int vx = (g.speed * bounceSin[index]) >> FP;
	ball->vx = angle < 0 ? -vx : vx;
	ball->vy = -((g.speed * bounceCos[index]) >> FP);
}

static void drawMessage(const char *text)
{
	int length = strlen(text);
	int column = FIELD_LEFT_COLUMN + (FIELD_RIGHT_COLUMN - FIELD_LEFT_COLUMN - length) / 2;
	drawText(column, MESSAGE_ROW, text);
}

static void clearMessage(void)
{
	clearRow(MESSAGE_ROW, FIELD_LEFT_COLUMN, FIELD_RIGHT_COLUMN - 1);
}

static void drawPanel(void)
{
	drawNumber(PANEL_COLUMN + 2, 6, g.score, 6);
	drawNumber(PANEL_COLUMN + 2, 10, g.highScore, 6);
	drawNumber(PANEL_COLUMN + 4, 14, g.level + 1, 2);
	for (int i = 0; i < MAX_LIVES; i++)
		setTile(PANEL_COLUMN + 1 + 2 * i, 18, i < g.lives ? TILE_LIFE : TILE_BLANK);
}

static void drawPanelLabels(void)
{
	drawText(PANEL_COLUMN, 1, "SHARD");
	drawText(PANEL_COLUMN + 3, 2, "BREAKER");
	drawText(PANEL_COLUMN + 2, 5, "SCORE");
	drawText(PANEL_COLUMN + 2, 9, "HIGH");
	drawText(PANEL_COLUMN + 2, 13, "LEVEL");
	drawText(PANEL_COLUMN + 2, 17, "LIVES");
	if (g.demo)
		drawText(PANEL_COLUMN + 2, 22, "DEMO");
}

static void addScore(uint32_t points)
{
	g.score += points;
	if (g.score > g.highScore && !g.demo)
		g.highScore = g.score;
}

static void clearObjects(void)
{
	memset(g.balls, 0, sizeof(g.balls));
	memset(g.capsules, 0, sizeof(g.capsules));
	memset(g.shots, 0, sizeof(g.shots));
	memset(g.wisps, 0, sizeof(g.wisps));
}

/* Put one ball on the paddle, ready to launch. */
static void serve(void)
{
	clearObjects();
	g.balls[0].active = 1;
	g.paddleMode = PADDLE_NORMAL;
	g.speed = SPEED_START + g.level * LEVEL_SPEED_STEP;
	if (g.speed > SPEED_MAX)
		g.speed = SPEED_MAX;
	g.paddleHits = 0;
	g.wispTimer = WISP_SPAWN_FRAMES;
	g.state = STATE_SERVE;
	g.timer = 0;
	drawMessage("READY");
}

static void startLevel(int level)
{
	g.level = level;
	g.remaining = loadLevel(level, g.bricks);
	drawFrame();
	drawPanelLabels();
	for (int row = 0; row < BRICK_ROWS; row++)
		for (int column = 0; column < BRICK_COLUMNS; column++)
			if (g.bricks[row][column] != BRICK_NONE)
				drawBrick(column, row, g.bricks[row][column]);
	g.paddleX = (FIELD_LEFT + FIELD_RIGHT - PADDLE_WIDTH) / 2;
	serve();
	playMusic(roundSong);
}

static void showTitle(void)
{
	g.state = STATE_TITLE;
	g.demo = 0;
	g.timer = DEMO_START_FRAMES;
	clearObjects();
	drawTitleScreen();
	drawText(14, TITLE_PROMPT_ROW + 2, "HIGH");
	drawNumber(20, TITLE_PROMPT_ROW + 2, g.highScore, 6);
	/* Two wisps drift around the title for decoration. */
	for (int i = 0; i < MAX_WISPS; i++)
	{
		g.wisps[i].active = 1;
		g.wisps[i].x = 40 + 200 * i;
		g.wisps[i].y = 130 - 60 * i;
		g.wisps[i].vx = i ? -1 : 1;
		g.wisps[i].vy = 1;
	}
	startSong(titleSong);
}

void gameInit(void)
{
	memset(&g, 0, sizeof(g));
	showTitle();
}

static void spawnCapsule(int brickColumn, int brickRow)
{
	if (rand() % CAPSULE_ODDS != 0)
		return;
	for (int i = 0; i < MAX_CAPSULES; i++)
	{
		object_t *c = &g.capsules[i];
		if (!c->active)
		{
			c->active = 1;
			c->type = rand() % CAPSULE_TYPES;
			c->x = FIELD_LEFT + brickColumn * BRICK_WIDTH;
			c->y = BRICK_TOP + brickRow * BRICK_HEIGHT;
			return;
		}
	}
}

static void hitBrick(int column, int row)
{
	uint8_t kind = g.bricks[row][column];
	if (kind == BRICK_GOLD)
	{
		fx(FX_STEEL);
		return;
	}
	if (kind == BRICK_STEEL)
	{
		g.bricks[row][column] = BRICK_STEEL_CRACKED;
		drawBrick(column, row, BRICK_STEEL_CRACKED);
		fx(FX_STEEL);
		return;
	}
	g.bricks[row][column] = BRICK_NONE;
	drawBrick(column, row, BRICK_NONE);
	g.remaining--;
	addScore(kind == BRICK_STEEL_CRACKED ? 100 : 40 + 10 * kind);
	fx(FX_BRICK);
	spawnCapsule(column, row);
}

/* If the pixel is inside a brick, hit the brick and return 1. */
static int hitBrickAt(int x, int y)
{
	x -= FIELD_LEFT;
	y -= BRICK_TOP;
	if (x < 0 || y < 0)
		return 0;
	int column = x / BRICK_WIDTH;
	int row = y / BRICK_HEIGHT;
	if (column >= BRICK_COLUMNS || row >= BRICK_ROWS || g.bricks[row][column] == BRICK_NONE)
		return 0;
	hitBrick(column, row);
	return 1;
}

/* If the ball overlaps a brick, hit it and return 1. */
static int collideBricks(const ball_t *ball)
{
	int x = ball->x >> FP;
	int y = ball->y >> FP;
	return hitBrickAt(x, y) || hitBrickAt(x + BALL_SIZE - 1, y)
		|| hitBrickAt(x, y + BALL_SIZE - 1) || hitBrickAt(x + BALL_SIZE - 1, y + BALL_SIZE - 1);
}

static void killWisp(object_t *wisp)
{
	wisp->active = 0;
	addScore(150);
	fx(FX_ENEMY);
}

/* A ball that touches a wisp destroys it and is knocked back. */
static void collideWisps(ball_t *ball)
{
	for (int i = 0; i < MAX_WISPS; i++)
	{
		object_t *w = &g.wisps[i];
		if (w->active && overlaps(ball->x >> FP, ball->y >> FP, BALL_SIZE, BALL_SIZE,
			w->x, w->y, WISP_SIZE, WISP_SIZE))
		{
			killWisp(w);
			ball->vy = -ball->vy;
		}
	}
}

/* Bounce the ball off the paddle, or catch it, if it has reached it. */
static void collidePaddle(ball_t *ball)
{
	int width = paddleWidth();
	int x = ball->x >> FP;
	int bottom = (ball->y >> FP) + BALL_SIZE;
	if (ball->vy <= 0 || bottom < PADDLE_Y || bottom > PADDLE_Y + PADDLE_CATCH_DEPTH)
		return;
	if (x + BALL_SIZE <= g.paddleX || x >= g.paddleX + width)
		return;
	/* The further from the centre, the flatter the bounce. */
	int half = width / 2;
	int offset = x + BALL_SIZE / 2 - (g.paddleX + half);
	int angle = offset * BOUNCE_ANGLES / (half + BALL_SIZE / 2);
	if (angle >= BOUNCE_ANGLES)
		angle = BOUNCE_ANGLES - 1;
	if (angle <= -BOUNCE_ANGLES)
		angle = -(BOUNCE_ANGLES - 1);
	if (angle == 0)
		angle = ball->vx < 0 ? -1 : 1;	/* never straight up */
	if (++g.paddleHits >= HITS_PER_SPEED_STEP && g.speed < SPEED_MAX)
	{
		g.paddleHits = 0;
		g.speed += SPEED_STEP;
	}
	aimBall(ball, angle);
	ball->y = (PADDLE_Y - BALL_SIZE) << FP;
	fx(FX_PADDLE);
	if (g.paddleMode == PADDLE_CATCH)
	{
		ball->held = 1;
		ball->holdOffset = x - g.paddleX;
		ball->holdTimer = CATCH_HOLD_FRAMES;
	}
	/* A new aim after every return keeps the demo from repeating one path. */
	g.demoAim = rand() % width;
}

static void moveBall(ball_t *ball)
{
	for (int step = 0; step < MOVE_STEPS; step++)
	{
		int dx = ball->vx / MOVE_STEPS;
		int dy = ball->vy / MOVE_STEPS;
		/* Moving one axis at a time tells us which side was hit. */
		ball->x += dx;
		if (ball->x < (FIELD_LEFT << FP))
		{
			ball->x = FIELD_LEFT << FP;
			ball->vx = -ball->vx;
			fx(FX_WALL);
		}
		else if (ball->x > ((FIELD_RIGHT - BALL_SIZE) << FP))
		{
			ball->x = (FIELD_RIGHT - BALL_SIZE) << FP;
			ball->vx = -ball->vx;
			fx(FX_WALL);
		}
		else if (collideBricks(ball))
		{
			ball->x -= dx;
			ball->vx = -ball->vx;
		}
		ball->y += dy;
		if (ball->y < (FIELD_TOP << FP))
		{
			ball->y = FIELD_TOP << FP;
			ball->vy = -ball->vy;
			fx(FX_WALL);
		}
		else if (collideBricks(ball))
		{
			ball->y -= dy;
			ball->vy = -ball->vy;
		}
		collidePaddle(ball);
		if (ball->held)
			return;
		if ((ball->y >> FP) >= FIELD_BOTTOM)
		{
			ball->active = 0;
			return;
		}
	}
	collideWisps(ball);
}

/* A caught ball rides on the paddle until fire is pressed or time runs out. */
static void holdBall(ball_t *ball, int release)
{
	int maxOffset = paddleWidth() - BALL_SIZE;
	if (ball->holdOffset > maxOffset)
		ball->holdOffset = maxOffset;
	ball->x = (g.paddleX + ball->holdOffset) << FP;
	ball->y = (PADDLE_Y - BALL_SIZE) << FP;
	if (release || --ball->holdTimer == 0)
	{
		ball->held = 0;
		fx(FX_LAUNCH);
	}
}

/* Turn one ball into three. */
static void splitBall(void)
{
	ball_t *source = NULL;
	for (int i = 0; i < MAX_BALLS && !source; i++)
		if (g.balls[i].active)
			source = &g.balls[i];
	if (!source)
		return;
	int angle = -3;
	for (int i = 0; i < MAX_BALLS; i++)
	{
		ball_t *ball = &g.balls[i];
		if (ball->active)
			continue;
		*ball = *source;
		ball->held = 0;
		aimBall(ball, angle);
		if (source->vy > 0)
			ball->vy = -ball->vy;
		angle += 6;
	}
}

static void setPaddleMode(int mode)
{
	g.paddleMode = mode;
	if (g.paddleX > FIELD_RIGHT - paddleWidth())
		g.paddleX = FIELD_RIGHT - paddleWidth();
	/* Only the catch paddle holds balls. */
	if (mode != PADDLE_CATCH)
		for (int i = 0; i < MAX_BALLS; i++)
			g.balls[i].held = 0;
}

static void applyCapsule(int type)
{
	fx(FX_CAPSULE);
	addScore(200);
	switch (type)
	{
		case CAPSULE_EXPAND:
			setPaddleMode(PADDLE_WIDE);
			break;
		case CAPSULE_LASER:
			setPaddleMode(PADDLE_LASER);
			break;
		case CAPSULE_CATCH:
			setPaddleMode(PADDLE_CATCH);
			break;
		case CAPSULE_SLOW:
			for (int i = 0; i < MAX_BALLS; i++)
			{
				g.balls[i].vx = g.balls[i].vx * SPEED_START / g.speed;
				g.balls[i].vy = g.balls[i].vy * SPEED_START / g.speed;
			}
			g.speed = SPEED_START;
			g.paddleHits = 0;
			break;
		case CAPSULE_MULTI:
			splitBall();
			break;
		case CAPSULE_LIFE:
			if (g.lives < MAX_LIVES)
				g.lives++;
			break;
	}
}

static void moveCapsules(void)
{
	for (int i = 0; i < MAX_CAPSULES; i++)
	{
		object_t *c = &g.capsules[i];
		if (!c->active)
			continue;
		c->y += CAPSULE_FALL_SPEED;
		if (overlaps(c->x, c->y, CAPSULE_WIDTH, CAPSULE_HEIGHT,
			g.paddleX, PADDLE_Y, paddleWidth(), PADDLE_HEIGHT))
		{
			c->active = 0;
			applyCapsule(c->type);
		}
		else if (c->y >= FIELD_BOTTOM)
		{
			c->active = 0;
		}
	}
}

/* Fire a pair of bolts from the ends of the laser paddle. */
static void fireLaser(void)
{
	object_t *pair[2] = {NULL, NULL};
	int found = 0;
	for (int i = 0; i < MAX_SHOTS && found < 2; i++)
		if (!g.shots[i].active)
			pair[found++] = &g.shots[i];
	if (found < 2)
		return;
	for (int i = 0; i < 2; i++)
	{
		pair[i]->active = 1;
		pair[i]->x = i ? g.paddleX + paddleWidth() - SHOT_INSET - SHOT_WIDTH : g.paddleX + SHOT_INSET;
		pair[i]->y = PADDLE_Y - SHOT_HEIGHT;
	}
	g.shotCooldown = SHOT_COOLDOWN_FRAMES;
	fx(FX_LASER);
}

static void moveShots(void)
{
	for (int i = 0; i < MAX_SHOTS; i++)
	{
		object_t *s = &g.shots[i];
		if (!s->active)
			continue;
		s->y -= SHOT_SPEED;
		if (s->y < FIELD_TOP || hitBrickAt(s->x, s->y) || hitBrickAt(s->x + SHOT_WIDTH - 1, s->y))
		{
			s->active = 0;
			continue;
		}
		for (int j = 0; j < MAX_WISPS; j++)
		{
			object_t *w = &g.wisps[j];
			if (w->active && overlaps(s->x, s->y, SHOT_WIDTH, SHOT_HEIGHT, w->x, w->y, WISP_SIZE, WISP_SIZE))
			{
				killWisp(w);
				s->active = 0;
			}
		}
	}
}

/* Wisps drift at half a pixel per frame inside the given box, turning now
   and then. They pass over bricks. */
static void driftWisps(int left, int top, int right, int bottom, int leaveAtBottom)
{
	if (g.frame & 1)
		return;
	for (int i = 0; i < MAX_WISPS; i++)
	{
		object_t *w = &g.wisps[i];
		if (!w->active)
			continue;
		if (rand() % WISP_TURN_ODDS == 0)
			w->vx = -w->vx;
		w->x += w->vx;
		w->y += w->vy;
		if (w->x < left || w->x > right - WISP_SIZE)
		{
			w->vx = -w->vx;
			w->x += 2 * w->vx;
		}
		if (w->y < top || (!leaveAtBottom && w->y > bottom - WISP_SIZE))
		{
			w->vy = -w->vy;
			w->y += 2 * w->vy;
		}
		if (w->y >= bottom)
			w->active = 0;
	}
}

static void updateWisps(void)
{
	if (--g.wispTimer == 0)
	{
		g.wispTimer = WISP_SPAWN_FRAMES;
		for (int i = 0; i < MAX_WISPS; i++)
		{
			object_t *w = &g.wisps[i];
			if (!w->active)
			{
				w->active = 1;
				w->x = FIELD_LEFT + rand() % (FIELD_RIGHT - FIELD_LEFT - WISP_SIZE);
				w->y = FIELD_TOP;
				w->vx = rand() & 1 ? 1 : -1;
				w->vy = 1;
				break;
			}
		}
	}
	driftWisps(FIELD_LEFT, FIELD_TOP, FIELD_RIGHT, FIELD_BOTTOM, 1);
	/* The paddle crushes a wisp that reaches it. */
	for (int i = 0; i < MAX_WISPS; i++)
	{
		object_t *w = &g.wisps[i];
		if (w->active && overlaps(w->x, w->y, WISP_SIZE, WISP_SIZE,
			g.paddleX, PADDLE_Y, paddleWidth(), PADDLE_HEIGHT))
			killWisp(w);
	}
}

static void movePaddle(int move)
{
	g.paddleX += move * PADDLE_MAX_SPEED / INPUT_MOVE_MAX;
	if (g.paddleX < FIELD_LEFT)
		g.paddleX = FIELD_LEFT;
	if (g.paddleX > FIELD_RIGHT - paddleWidth())
		g.paddleX = FIELD_RIGHT - paddleWidth();
}

/* `fire` is the press of the button, `fireHeld` its current state. */
static void play(int fire, int fireHeld)
{
	int alive = 0;
	if (g.shotCooldown)
		g.shotCooldown--;
	if (g.paddleMode == PADDLE_LASER && fireHeld && !g.shotCooldown)
		fireLaser();
	for (int i = 0; i < MAX_BALLS; i++)
	{
		ball_t *ball = &g.balls[i];
		if (ball->active && ball->held)
			holdBall(ball, fire);
		else if (ball->active)
			moveBall(ball);
		alive += ball->active;
	}
	moveCapsules();
	moveShots();
	updateWisps();
	if (g.remaining == 0)
	{
		/* After the last layout the levels start over, faster. */
		int roundDone = (g.level + 1) % NUM_LEVELS == 0;
		g.state = STATE_CLEAR;
		g.timer = roundDone ? ALL_CLEAR_PAUSE_FRAMES : CLEAR_PAUSE_FRAMES;
		clearObjects();
		addScore(roundDone ? ALL_CLEAR_BONUS : LEVEL_BONUS);
		if (roundDone)
		{
			drawMessage("ALL CLEAR  BONUS 10000");
			playMusic(victorySong);
		}
		else
		{
			drawMessage("LEVEL CLEAR");
			fx(FX_CLEAR);
		}
	}
	else if (!alive)
	{
		g.state = STATE_LOST;
		g.timer = LOST_PAUSE_FRAMES;
		memset(g.capsules, 0, sizeof(g.capsules));
		memset(g.shots, 0, sizeof(g.shots));
		memset(g.wisps, 0, sizeof(g.wisps));
		fx(FX_LOSE);
	}
}

/* Queue or remove one sprite slot. */
static void sprite(int slot, int active, int x, int y, int frame)
{
	if (active)
		putSprite(slot, x, y, 0, frame);
	else
		removeSprite(slot);
}

static void queueSprites(void)
{
	sprite(SPRITE_PADDLE, g.state != STATE_TITLE, g.paddleX, PADDLE_Y, FRAME_PADDLE + g.paddleMode);
	for (int i = 0; i < MAX_CAPSULES; i++)
	{
		const object_t *c = &g.capsules[i];
		sprite(SPRITE_CAPSULE + i, c->active, c->x, c->y, FRAME_CAPSULE_E + c->type);
	}
	for (int i = 0; i < MAX_SHOTS; i++)
	{
		const object_t *s = &g.shots[i];
		sprite(SPRITE_SHOT + i, s->active, s->x, s->y, FRAME_SHOT);
	}
	int wispFrame = FRAME_WISP_0 + ((g.frame / WISP_ANIMATION_FRAMES) & 1);
	for (int i = 0; i < MAX_WISPS; i++)
	{
		const object_t *w = &g.wisps[i];
		sprite(SPRITE_WISP + i, w->active, w->x, w->y, wispFrame);
	}
	for (int i = 0; i < MAX_BALLS; i++)
	{
		const ball_t *ball = &g.balls[i];
		sprite(SPRITE_BALL + i, ball->active, ball->x >> FP, ball->y >> FP, FRAME_BALL);
	}
}

/* In the demo the paddle follows the lowest ball, serves by itself and
   keeps the fire button going. */
static void demoInput(input_t *input)
{
	const ball_t *target = NULL;
	for (int i = 0; i < MAX_BALLS; i++)
		if (g.balls[i].active && (!target || g.balls[i].y > target->y))
			target = &g.balls[i];
	input->move = 0;
	input->fire = 0;
	input->pause = 0;
	input->cheat = 0;
	if (g.state == STATE_SERVE)
	{
		input->fire = ++g.timer >= DEMO_SERVE_FRAMES;
		return;
	}
	input->fire = (g.frame & 31) < 16;
	if (!target)
		return;
	int wanted = (target->x >> FP) + BALL_SIZE / 2 - g.demoAim;
	int distance = wanted - g.paddleX;
	if (distance > PADDLE_MAX_SPEED)
		input->move = INPUT_MOVE_MAX;
	else if (distance < -PADDLE_MAX_SPEED)
		input->move = -INPUT_MOVE_MAX;
}

static void updateTitle(int fire)
{
	driftWisps(0, 0, SCREEN_COLUMNS * 8, SCREEN_ROWS * 8, 0);
	if (g.frame % TITLE_BLINK_FRAMES == 0)
	{
		if ((g.frame / TITLE_BLINK_FRAMES) & 1)
			clearRow(TITLE_PROMPT_ROW, 15, 24);
		else
			drawText(15, TITLE_PROMPT_ROW, "PRESS FIRE");
	}
	if (fire || --g.timer == 0)
	{
		stopSong();
		g.demo = !fire;
		g.score = 0;
		g.lives = g.demo ? 1 : START_LIVES;
		startLevel(g.demo ? rand() % NUM_LEVELS : 0);
	}
}

void gameUpdate(const input_t *playerInput)
{
	input_t demo;
	const input_t *input = playerInput;
	int fire = playerInput->fire && !g.lastFire;	/* only on the press */
	g.lastFire = playerInput->fire;
	g.frame++;
	/* Pause freezes a game in progress; the picture stays as it is. */
	int pausePressed = playerInput->pause && !g.lastPause;
	g.lastPause = playerInput->pause;
	if (pausePressed && !g.demo && (g.state == STATE_SERVE || g.state == STATE_PLAY))
	{
		g.paused = !g.paused;
		clearMessage();
		if (g.paused)
			drawMessage("PAUSED");
		else if (g.state == STATE_SERVE)
			drawMessage("READY");
	}
	if (g.paused)
	{
		queueSprites();
		return;
	}
	/* Testing aids, only ever set by emulator builds. */
	int cheat = playerInput->cheat != g.lastCheat ? playerInput->cheat : 0;
	g.lastCheat = playerInput->cheat;
	if (cheat == CHEAT_AUTOPILOT)
		g.autopilot = !g.autopilot;
	if (g.autopilot && !g.demo && (g.state == STATE_SERVE || g.state == STATE_PLAY))
	{
		static uint8_t lastAutoFire;
		demoInput(&demo);
		input = &demo;
		fire = demo.fire && !lastAutoFire;
		lastAutoFire = demo.fire;
	}
	if (g.demo)
	{
		if (fire)
		{
			showTitle();
			fire = 0;
		}
		else
		{
			static uint8_t lastDemoFire;
			demoInput(&demo);
			input = &demo;
			fire = demo.fire && !lastDemoFire;
			lastDemoFire = demo.fire;
		}
	}

	switch (g.state)
	{
		case STATE_TITLE:
			updateTitle(fire);
			break;
		case STATE_SERVE:
			movePaddle(input->move);
			g.balls[0].x = (g.paddleX + paddleWidth() / 2 - BALL_SIZE / 2) << FP;
			g.balls[0].y = (PADDLE_Y - BALL_SIZE) << FP;
			if (fire)
			{
				clearMessage();
				g.timer = 0;
				aimBall(&g.balls[0], 2);
				fx(FX_LAUNCH);
				g.state = STATE_PLAY;
			}
			break;
		case STATE_PLAY:
			movePaddle(input->move);
			if (cheat == CHEAT_NEXT_LEVEL)
				g.remaining = 0;	/* the level counts as cleared */
			if (cheat >= 1 && cheat <= CAPSULE_TYPES)
				applyCapsule(cheat - 1);
			play(fire, input->fire);
			break;
		case STATE_LOST:
			if (--g.timer == 0)
			{
				if (--g.lives > 0)
				{
					serve();
				}
				else if (g.demo)
				{
					showTitle();
				}
				else
				{
					g.state = STATE_GAME_OVER;
					drawMessage("GAME OVER");
					playMusic(gameoverSong);
				}
			}
			break;
		case STATE_CLEAR:
			if (--g.timer == 0)
				startLevel(g.level + 1);
			break;
		case STATE_GAME_OVER:
			if (fire)
				showTitle();
			break;
	}
	if (g.state != STATE_TITLE)
		drawPanel();
	queueSprites();
}

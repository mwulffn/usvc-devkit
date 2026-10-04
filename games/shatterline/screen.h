#ifndef SCREEN_H_
#define SCREEN_H_
#include <stdint.h>

/* Screen layout, in 8-pixel tiles. */
#define SCREEN_COLUMNS 40
#define SCREEN_ROWS 25
#define FIELD_LEFT_COLUMN 1		/* first column inside the left wall */
#define FIELD_RIGHT_COLUMN 27	/* the right wall */
#define FIELD_TOP_ROW 1			/* first row below the top wall */
#define BRICK_TOP_ROW 3
#define PANEL_COLUMN 29

/* The same in pixels. */
#define FIELD_LEFT (FIELD_LEFT_COLUMN * 8)
#define FIELD_RIGHT (FIELD_RIGHT_COLUMN * 8)
#define FIELD_TOP (FIELD_TOP_ROW * 8)
#define FIELD_BOTTOM 200
#define BRICK_TOP (BRICK_TOP_ROW * 8)
#define BRICK_WIDTH 16
#define BRICK_HEIGHT 8

void screenInit(void);
void setTile(int column, int row, int tile);
void drawText(int column, int row, const char *text);
void drawNumber(int column, int row, uint32_t value, int digits);
void drawFieldBackground(int column, int row);
void drawBrick(int brickColumn, int brickRow, uint8_t kind);
void drawFrame(void);
void drawTitleScreen(void);
void clearRow(int row, int firstColumn, int lastColumn);
#endif /* SCREEN_H_ */

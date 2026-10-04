/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright (C) 2026 Michael Wulff Nielsen */
/* Tile drawing: the playfield frame, bricks, background and text. */
#include "main.h"

/* Font tiles follow the artwork tiles in RAM: one per character ' ' to 'Z'. */
#define FONT_FIRST_CHAR ' '
#define FONT_LAST_CHAR 'Z'
#define FONT_TILE_BASE NUM_ROM_TILES
#define FONT_TILES (FONT_LAST_CHAR - FONT_FIRST_CHAR + 1)
#define TEXT_COLOUR 0xFF		/* white */

_Static_assert(NUM_ROM_TILES + FONT_TILES <= GAME_RAM_TILES,
	"raise GAME_RAM_TILES in usvc_config.h");

void screenInit(void)
{
	memcpy(tiles, tileData, sizeof(tileData));
	for (int c = FONT_FIRST_CHAR; c <= FONT_LAST_CHAR; c++)
	{
		uint8_t *tile = (uint8_t *) tiles[FONT_TILE_BASE + c - FONT_FIRST_CHAR];
		memset(tile, 0, 64);
		putCharInTile(NULL, c, TEXT_COLOUR, 0, 0, tile, 0);
	}
	setNumberOfRamTiles(GAME_RAM_TILES);
}

void setTile(int column, int row, int tile)
{
	placeTile(column, row, tile);
}

/* Upper-case letters, digits and punctuation up to 'Z'; the rest are blank. */
void drawText(int column, int row, const char *text)
{
	for (; *text && column < SCREEN_COLUMNS; text++, column++)
	{
		char c = *text;
		if (c >= 'a' && c <= 'z')
			c -= 'a' - 'A';
		if (c < FONT_FIRST_CHAR || c > FONT_LAST_CHAR)
			c = ' ';
		setTile(column, row, FONT_TILE_BASE + c - FONT_FIRST_CHAR);
	}
}

void drawNumber(int column, int row, uint32_t value, int digits)
{
	for (int i = digits - 1; i >= 0; i--)
	{
		setTile(column + i, row, FONT_TILE_BASE + '0' - FONT_FIRST_CHAR + value % 10);
		value /= 10;
	}
}

/* The background is a 16x16 pattern made of four tiles. */
void drawFieldBackground(int column, int row)
{
	setTile(column, row, TILE_BG_0 + (column & 1) + 2 * (row & 1));
}

void drawBrick(int brickColumn, int brickRow, uint8_t kind)
{
	int column = FIELD_LEFT_COLUMN + 2 * brickColumn;
	int row = BRICK_TOP_ROW + brickRow;
	if (kind == BRICK_NONE)
	{
		drawFieldBackground(column, row);
		drawFieldBackground(column + 1, row);
		return;
	}
	/* Brick tiles come in left/right pairs in the order of the kinds. */
	int tile = TILE_BRICK_RED_L + 2 * (kind - 1);
	setTile(column, row, tile);
	setTile(column + 1, row, tile + 1);
}

/* Walls, empty playfield and a blank side panel. */
void drawFrame(void)
{
	for (int row = 0; row < SCREEN_ROWS; row++)
	{
		for (int column = 0; column < SCREEN_COLUMNS; column++)
		{
			int side = column == 0 || column == FIELD_RIGHT_COLUMN;
			if (column > FIELD_RIGHT_COLUMN)
				setTile(column, row, TILE_BLANK);
			else if (row == 0)
				setTile(column, row, side ? TILE_WALL_CORNER : TILE_WALL_TOP);
			else if (side)
				setTile(column, row, TILE_WALL_SIDE);
			else
				drawFieldBackground(column, row);
		}
	}
}

void clearRow(int row, int firstColumn, int lastColumn)
{
	for (int column = firstColumn; column <= lastColumn; column++)
		drawFieldBackground(column, row);
}

/*
 * The title logo is built from five block shapes per colour. In the letter
 * shapes below: '#' a full block, '.' nothing, and 'a' 'b' 'c' 'd' a block
 * with its top-left, top-right, bottom-left or bottom-right corner cut off.
 */
#define LOGO_ROWS 5
#define LOGO_SHAPES 5
static const char *logoLetter(char c)
{
	switch (c)
	{
		case 'S': return "a##" "#.." "c#b" "..#" "##d";
		case 'H': return "#.#" "#.#" "###" "#.#" "#.#";
		case 'A': return "a#b" "#.#" "###" "#.#" "#.#";
		case 'R': return "##b" "#.#" "##d" "#b." "#.#";
		case 'D': return "##b" "#.#" "#.#" "#.#" "##d";
		case 'B': return "##b" "#.#" "##." "#.#" "##d";
		case 'K': return "#.#" "#.d" "##." "#.b" "#.#";
		default:  return "###" "#.." "##." "#.." "###";	/* E */
	}
}

/* Draw `word` in big letters; `firstTile` is the colour's full block tile. */
static void drawLogoWord(const char *word, int column, int row, int firstTile)
{
	static const char shapes[LOGO_SHAPES + 1] = "#abcd";
	for (; *word; word++, column += 4)
	{
		const char *letter = logoLetter(*word);
		for (int i = 0; i < 3 * LOGO_ROWS; i++)
		{
			const char *shape = strchr(shapes, letter[i]);
			if (shape)
				setTile(column + i % 3, row + i / 3, firstTile + (shape - shapes));
		}
	}
}

/* The whole screen, without the playfield frame. */
void drawTitleScreen(void)
{
	for (int row = 0; row < SCREEN_ROWS; row++)
		clearRow(row, 0, SCREEN_COLUMNS - 1);
	drawLogoWord("SHARD", 8, 3, TILE_LOGO_A_FULL);
	drawLogoWord("BREAKER", 7, 10, TILE_LOGO_B_FULL);
	drawText(5, 22, "A D OR STICK: MOVE   SPACE: FIRE");
}

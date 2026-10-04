/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright (C) 2026 Michael Wulff Nielsen */
/*
 * Shardbreaker, a brick-breaking game for the uSVC console.
 * This file: start-up, the frame loop and input from keyboard and gamepad.
 */
#include "main.h"

/* Stick movement below this fraction of full deflection is ignored. */
#define STICK_DEADZONE_DIVISOR 6
#define GAMEPAD_FIRE_BUTTONS (GP_BUTTON_1 | GP_BUTTON_2 | GP_BUTTON_3 | GP_BUTTON_4 | GP_BUTTON_START)
/* The kernel reports non-printing keys as their USB code in the high byte. */
#define KEY_LEFT (USB_KEY_LEFT << 8)
#define KEY_RIGHT (USB_KEY_RIGHT << 8)
/* Last scan line of the frame; input is polled until then. */
#define LAST_POLL_LINE 523

static void readKeyboard(input_t *input)
{
	uint16_t keys[6];
	usbKeyboardPoll();
	usbGetCurrentAsciiKeyboardStateEx(keys);
	for (int i = 0; i < 6; i++)
	{
		switch (keys[i])
		{
			case 'a': case 'A': case KEY_LEFT:
				input->move = -INPUT_MOVE_MAX;
				break;
			case 'd': case 'D': case KEY_RIGHT:
				input->move = INPUT_MOVE_MAX;
				break;
			case ' ': case '\r':
				input->fire = 1;
				break;
			case 'p': case 'P':
				input->pause = 1;
				break;
#ifdef USVC_EMULATOR
			/* Testing aids: keys 1-6 hand out a capsule (E S M L Z C), 9 skips
			   to the next level, 0 switches the autopilot on or off. */
			case '1': case '2': case '3': case '4': case '5': case '6': case '9':
				input->cheat = keys[i] - '0';
				break;
			case '0':
				input->cheat = CHEAT_AUTOPILOT;
				break;
#endif
		}
	}
}

static void readGamepad(input_t *input)
{
	gamePadState_t pad;
	usbHidGenericGamepadPoll();
	if (!getCurrentGamepadState(&pad))
		return;
	int range = (pad.XYZRxMaximum - pad.XYZRxMinimum) / 2;
	int offset = pad.axes[0] - (pad.XYZRxMinimum + range);
	if (range > 0 && (offset > range / STICK_DEADZONE_DIVISOR || offset < -range / STICK_DEADZONE_DIVISOR))
	{
		int move = offset * INPUT_MOVE_MAX / range;
		if (move > INPUT_MOVE_MAX)
			move = INPUT_MOVE_MAX;
		if (move < -INPUT_MOVE_MAX)
			move = -INPUT_MOVE_MAX;
		input->move = move;
	}
	if (pad.buttons & GAMEPAD_FIRE_BUTTONS)
		input->fire = 1;
	if (pad.buttons & GP_BUTTON_SELECT)
		input->pause = 1;
}

/* Service USB for the rest of the frame and return the latest input. */
static void pollInput(input_t *input)
{
	do
	{
		input->move = 0;
		input->fire = 0;
		input->pause = 0;
		input->cheat = 0;
		usbHostTask();
		if (usbHidBootKeyboardIsInstalled())
			readKeyboard(input);
		if (usbHidGenericGamepadIsInstalled())
			readGamepad(input);
	} while (getCurrentScanLineNumber() < LAST_POLL_LINE);
}

int main(void)
{
	input_t input = {0, 0, 0, 0};
	initUsvc(patches);
	screenInit();
	gameInit();
	usvcDebugPrint("shardbreaker: started\n");
	while (1)
	{
		/* Everything that changes the picture happens in the vertical blank. */
		waitForVerticalBlank();
		soundEngine();
		restoreBackgroundTiles();
		gameUpdate(&input);
		drawSprites();
#ifdef USVC_EMULATOR
		/* Report if a frame's work ever runs past the vertical blank. */
		static uint32_t worstLine;
		uint32_t line = getCurrentScanLineNumber();
		if (line > worstLine && line >= 401)
		{
			char text[40];
			worstLine = line;
			snprintf(text, sizeof(text), "frame work ended at line %lu\n", (unsigned long) line);
			usvcDebugPrint(text);
		}
#endif
		pollInput(&input);
	}
}

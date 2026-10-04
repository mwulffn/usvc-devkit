/*
 * Hello uSVC: the smallest useful game skeleton.
 * Prints text in bitmapped mode, counts seconds and shows the last key.
 */
#include "main.h"

/* The four colours a pixel can have, as (red 0-7, green 0-7, blue 0-3). */
static const uint8_t colours[4][3] =
{
	{0, 0, 1},	/* 0: background, dark blue */
	{7, 7, 3},	/* 1: white */
	{7, 5, 0},	/* 2: orange */
	{0, 7, 1},	/* 3: green */
};

/* A palette entry describes two neighbouring pixels at once. */
static void initPalette(void)
{
	for (int a = 0; a < 4; a++)
	{
		for (int b = 0; b < 4; b++)
		{
			palette[(a << 2) | b] = BICOLOR(colours[a][0], colours[a][1], colours[a][2],
				colours[b][0], colours[b][1], colours[b][2]);
		}
	}
}

int main(void)
{
	char text[41];
	uint16_t keys[6];
	uint32_t seconds = 0;
	uint32_t lastTime;

	initPalette();
	initUsvc(NULL);
	lastTime = millis();
	usvcDebugPrint("hello: started\n");

	printText(NULL, "Hello, uSVC!", 14, 8, 1, 0);
	printText(NULL, "Press a key", 14, 12, 2, 0);
	while (1)
	{
		waitForVerticalBlank();
		uint32_t timeNow = millis();
		if (timeNow - lastTime >= 1000UL)
		{
			lastTime += 1000UL;
			seconds++;
			setLed(seconds & 1);
			snprintf(text, sizeof(text), "Seconds: %lu", (unsigned long) seconds);
			printText(NULL, text, 14, 16, 3, 0);
		}
		/* Input: let the USB host run, then read the keyboard. */
		do
		{
			usbHostTask();
			if (usbHidBootKeyboardIsInstalled())
			{
				usbKeyboardPoll();
				usbGetCurrentAsciiKeyboardStateEx(keys);
				if (keys[0] >= ' ' && keys[0] < 127)
				{
					snprintf(text, sizeof(text), "Key: %c", (char) keys[0]);
					printText(NULL, text, 14, 20, 1, 0);
				}
			}
		} while (getCurrentScanLineNumber() < 523);
	}
}

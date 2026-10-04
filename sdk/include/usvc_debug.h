/* SPDX-License-Identifier: GPL-3.0-or-later */
/* Copyright (C) 2026 Michael Wulff Nielsen */
/*
 * Debug text output for games running in the emulator.
 *
 * Build with EMULATOR=1 (defines USVC_EMULATOR) to enable it. In a normal
 * build these functions do nothing, because the debug port address is not
 * mapped on real hardware and writing to it would fault.
 */
#ifndef USVC_DEBUG_H_
#define USVC_DEBUG_H_
#include <stdint.h>

#define USVC_DEBUG_PORT 0x42005400UL

static inline void usvcDebugPutc(char c)
{
#ifdef USVC_EMULATOR
	*(volatile uint8_t *) USVC_DEBUG_PORT = (uint8_t) c;
#else
	(void) c;
#endif
}

static inline void usvcDebugPrint(const char *text)
{
	while (*text)
		usvcDebugPutc(*text++);
}

#endif /* USVC_DEBUG_H_ */

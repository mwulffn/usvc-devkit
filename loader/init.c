// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Michael Wulff Nielsen

// The upstream reset handler calls __libc_init_array() to run constructors.
// The loader is plain C and has none, so this empty one replaces the C
// library's, which would pull in the start-up files (about 250 bytes).
void __libc_init_array(void)
{
}

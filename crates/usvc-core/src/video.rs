// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Michael Wulff Nielsen
//! Rebuilds the picture from writes to the upper half of the PORT `OUT`
//! register, the way a monitor would: by time since the sync pulses.

pub const SCREEN_WIDTH: usize = 320;
pub const SCREEN_HEIGHT: usize = 400;

/// CPU cycles per pixel in every video mode.
const CYCLES_PER_PIXEL: i64 = 4;
/// Cycle within a line (from the horizontal timer overflow) of pixel 0.
const DEFAULT_X0: i64 = 274;
/// How far the first write of a line may be from `x0` and still count as
/// pixel 0.
const MAX_PHASE_SHIFT: i64 = 8;
/// Lines after vertical sync of the first visible line.
const DEFAULT_Y0: i64 = 76;

/// Observed timing, for calibration and diagnostics.
#[derive(Clone, Copy, Debug, Default)]
pub struct VideoDebug {
    /// Earliest cycle within a line of a non-black colour write, last frame.
    pub first_x: Option<u32>,
    /// First line after vertical sync with a colour write, last frame.
    pub first_line: Option<u32>,
    /// Last such line.
    pub last_line: Option<u32>,
}

pub struct Video {
    pub fb: Vec<u32>,
    pub debug: VideoDebug,
    pub x0: i64,
    pub y0: i64,
    lut: [u32; 256],
    /// Colour currently on the pins (upper half of `OUT`).
    cur: u16,
    /// Lines since vertical sync.
    line: i64,
    line_start: u64,
    /// Pixels of the current line already written.
    filled: usize,
    /// Whether the current line has had a write, and its pixel 0 cycle.
    line_seen: bool,
    line_x0: i64,
    acc: VideoDebug,
}

/// Pack the eight colour pins of the upper port half into one byte.
#[inline]
fn pins_to_index(hi: u16) -> usize {
    ((hi & 0xCF) | (hi >> 10 & 0x30)) as usize
}

fn build_lut() -> [u32; 256] {
    // Pin map from the schematic: PA16=R0, PA18=R1, PA17=R2, PA30=G0,
    // PA22=G1, PA23=G2, PA19=B0, PA31=B1. Index bits 4 and 5 hold PA30, PA31.
    let mut lut = [0u32; 256];
    for (i, entry) in lut.iter_mut().enumerate() {
        let i = i as u32;
        let r = (i & 1) | (i >> 2 & 1) << 1 | (i >> 1 & 1) << 2;
        let g = (i >> 4 & 1) | (i >> 6 & 1) << 1 | (i >> 7 & 1) << 2;
        let b = (i >> 3 & 1) | (i >> 5 & 1) << 1;
        *entry = (r * 255 / 7) << 16 | (g * 255 / 7) << 8 | (b * 255 / 3);
    }
    lut
}

impl Video {
    pub fn new() -> Self {
        Video {
            fb: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT],
            debug: VideoDebug::default(),
            x0: DEFAULT_X0,
            y0: DEFAULT_Y0,
            lut: build_lut(),
            cur: 0,
            line: 0,
            line_start: 0,
            filled: 0,
            line_seen: false,
            line_x0: DEFAULT_X0,
            acc: VideoDebug::default(),
        }
    }

    fn fill_to(&mut self, x: usize) {
        let row = self.line - self.y0;
        if (0..SCREEN_HEIGHT as i64).contains(&row) && x > self.filled {
            let base = row as usize * SCREEN_WIDTH;
            let colour = self.lut[pins_to_index(self.cur)];
            self.fb[base + self.filled..base + x].fill(colour);
        }
        self.filled = self.filled.max(x);
    }

    /// The upper half of `OUT` was written at `cycle`.
    pub fn port_write(&mut self, cycle: u64, hi: u16) {
        let lc = cycle as i64 - self.line_start as i64;
        if !self.line_seen {
            // The video modes start a line a few cycles apart. Take the first
            // write near the expected position as pixel 0 of this line.
            self.line_seen = true;
            self.line_x0 = if (lc - self.x0).abs() <= MAX_PHASE_SHIFT {
                lc
            } else {
                self.x0
            };
        }
        let x = (lc - self.line_x0 + CYCLES_PER_PIXEL / 2).div_euclid(CYCLES_PER_PIXEL);
        self.fill_to(x.clamp(0, SCREEN_WIDTH as i64) as usize);
        self.cur = hi;
        if pins_to_index(hi) != 0 {
            let line = self.line as u32;
            let lc = lc.max(0) as u32;
            self.acc.first_x = Some(self.acc.first_x.map_or(lc, |v| v.min(lc)));
            self.acc.first_line.get_or_insert(line);
            self.acc.last_line = Some(line);
        }
    }

    /// Start of a new line (horizontal timer overflow).
    pub fn hsync(&mut self, cycle: u64) {
        self.fill_to(SCREEN_WIDTH);
        self.line += 1;
        self.line_start = cycle;
        self.filled = 0;
        self.line_seen = false;
        self.line_x0 = self.x0;
    }

    /// Start of a new frame (vertical timer overflow).
    pub fn vsync(&mut self) {
        self.line = 0;
        self.debug = std::mem::take(&mut self.acc);
    }
}

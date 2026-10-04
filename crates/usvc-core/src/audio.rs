// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Michael Wulff Nielsen
//! Audio helpers for front ends.

/// Removes the constant offset of the DAC signal, as the AC coupling of the
/// console's audio output does. The kernel centres its output at a quarter
/// of the DAC range, so raw samples sit well below zero.
#[derive(Clone, Copy, Debug, Default)]
pub struct DcBlocker {
    prev_in: i32,
    prev_out: i32,
}

impl DcBlocker {
    /// Filter `samples` in place (a one-pole high-pass at roughly 5 Hz).
    pub fn process(&mut self, samples: &mut [i16]) {
        // y[n] = k * (y[n-1] + x[n] - x[n-1]), k = 1023/1024, in 22.10 fixed point.
        for s in samples {
            let x = (*s as i32) << 10;
            let y = (self.prev_out + x - self.prev_in) / 1024 * 1023;
            self.prev_in = x;
            self.prev_out = y;
            *s = (y >> 10).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_constant_offset_and_keeps_the_signal() {
        let mut dc = DcBlocker::default();
        // A square wave of +-1000 around -12000, three seconds at 30 kHz.
        let mut samples: Vec<i16> = (0..90_000)
            .map(|i| -12_000 + if (i / 50) % 2 == 0 { 1000 } else { -1000 })
            .collect();
        dc.process(&mut samples);
        let tail = &samples[60_000..];
        let mean = tail.iter().map(|s| *s as i64).sum::<i64>() / tail.len() as i64;
        let peak = tail.iter().map(|s| s.abs()).max().unwrap();
        assert!(mean.abs() < 50, "mean {mean}");
        assert!((900..=1100).contains(&peak), "peak {peak}");
    }
}

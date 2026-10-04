//! Input scripts: timed key and gamepad events for a headless run.
//!
//! One event per line, `#` starts a comment:
//!
//! ```text
//! 60 tap ENTER          # press for 3 frames
//! 90 tap DOWN 10        # press for 10 frames
//! 120 press LEFT
//! 150 release LEFT
//! 200 pad 0x0001 0 127  # buttons, then optional X and Y axis values
//! ```

use usvc_core::keys::{key_from_name, Key};
use usvc_core::{GamepadState, KeyboardState, Machine};

const DEFAULT_TAP_FRAMES: u32 = 3;

#[derive(Clone, Copy, Debug)]
enum Action {
    Press(Key),
    Release(Key),
    Pad(GamepadState),
}

#[derive(Default)]
pub struct Script {
    /// Sorted by frame.
    events: Vec<(u32, Action)>,
    next: usize,
    keyboard: KeyboardState,
    pub uses_gamepad: bool,
}

fn parse_int(s: &str) -> Option<i64> {
    match s.strip_prefix("0x") {
        Some(hex) => i64::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    }
}

impl Script {
    pub fn parse(text: &str) -> Result<Script, String> {
        let mut script = Script::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            script
                .parse_line(line)
                .map_err(|e| format!("line {}: {e}", n + 1))?;
        }
        script.events.sort_by_key(|(f, _)| *f);
        Ok(script)
    }

    /// Add a tap given on the command line as `FRAME:KEY` or
    /// `FRAME:KEY:FRAMES` (how long to hold it).
    pub fn add_tap(&mut self, spec: &str) -> Result<(), String> {
        let parts: Vec<&str> = spec.split(':').collect();
        let line = match parts[..] {
            [frame, key] => format!("{frame} tap {key}"),
            [frame, key, hold] => format!("{frame} tap {key} {hold}"),
            _ => return Err(format!("expected FRAME:KEY[:FRAMES], got \"{spec}\"")),
        };
        self.parse_line(&line)?;
        self.events.sort_by_key(|(f, _)| *f);
        Ok(())
    }

    fn parse_line(&mut self, line: &str) -> Result<(), String> {
        let words: Vec<&str> = line.split_whitespace().collect();
        let frame: u32 = words[0]
            .parse()
            .map_err(|_| format!("bad frame number \"{}\"", words[0]))?;
        let key = |i: usize| -> Result<Key, String> {
            let name = words.get(i).ok_or("missing key name")?;
            key_from_name(name).ok_or_else(|| format!("unknown key \"{name}\""))
        };
        match words.get(1).copied() {
            Some("press") => self.events.push((frame, Action::Press(key(2)?))),
            Some("release") => self.events.push((frame, Action::Release(key(2)?))),
            Some("tap") => {
                let k = key(2)?;
                let hold = match words.get(3) {
                    Some(w) => w.parse().map_err(|_| format!("bad frame count \"{w}\""))?,
                    None => DEFAULT_TAP_FRAMES,
                };
                self.events.push((frame, Action::Press(k)));
                self.events.push((frame + hold, Action::Release(k)));
            }
            Some("pad") => {
                let num = |i: usize| -> Result<Option<i64>, String> {
                    words
                        .get(i)
                        .map(|w| parse_int(w).ok_or_else(|| format!("bad number \"{w}\"")))
                        .transpose()
                };
                let mut pad = GamepadState {
                    buttons: num(2)?.ok_or("missing button mask")? as u32,
                    ..GamepadState::default()
                };
                if let Some(x) = num(3)? {
                    pad.axes[0] = x as i16;
                }
                if let Some(y) = num(4)? {
                    pad.axes[1] = y as i16;
                }
                self.uses_gamepad = true;
                self.events.push((frame, Action::Pad(pad)));
            }
            other => return Err(format!("unknown action \"{}\"", other.unwrap_or(""))),
        }
        Ok(())
    }

    /// Apply the events due at `frame` (call before running that frame).
    pub fn apply(&mut self, frame: u32, m: &mut Machine) {
        let mut keys_changed = false;
        while let Some((f, action)) = self.events.get(self.next) {
            if *f > frame {
                break;
            }
            match *action {
                Action::Press(k) => {
                    self.keyboard.press(k);
                    keys_changed = true;
                }
                Action::Release(k) => {
                    self.keyboard.release(k);
                    keys_changed = true;
                }
                Action::Pad(p) => m.set_gamepad(p),
            }
            self.next += 1;
        }
        if keys_changed {
            m.set_keyboard(self.keyboard);
        }
    }
}

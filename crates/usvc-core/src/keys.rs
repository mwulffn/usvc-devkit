// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Michael Wulff Nielsen
//! Key names for front ends and input scripts, as USB HID usage codes.

/// A key on a USB keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// HID usage code of an ordinary key.
    Usage(u8),
    /// Bit in the modifier byte (control, shift, alt, GUI; left then right).
    Modifier(u8),
}

const NAMED: &[(&str, Key)] = &[
    ("ENTER", Key::Usage(40)),
    ("RETURN", Key::Usage(40)),
    ("ESC", Key::Usage(41)),
    ("ESCAPE", Key::Usage(41)),
    ("BACKSPACE", Key::Usage(42)),
    ("TAB", Key::Usage(43)),
    ("SPACE", Key::Usage(44)),
    ("MINUS", Key::Usage(45)),
    ("EQUAL", Key::Usage(46)),
    ("COMMA", Key::Usage(54)),
    ("DOT", Key::Usage(55)),
    ("SLASH", Key::Usage(56)),
    ("DELETE", Key::Usage(76)),
    ("RIGHT", Key::Usage(79)),
    ("LEFT", Key::Usage(80)),
    ("DOWN", Key::Usage(81)),
    ("UP", Key::Usage(82)),
    ("LCTRL", Key::Modifier(0x01)),
    ("LSHIFT", Key::Modifier(0x02)),
    ("LALT", Key::Modifier(0x04)),
    ("RCTRL", Key::Modifier(0x10)),
    ("RSHIFT", Key::Modifier(0x20)),
    ("RALT", Key::Modifier(0x40)),
    ("CTRL", Key::Modifier(0x01)),
    ("SHIFT", Key::Modifier(0x02)),
    ("ALT", Key::Modifier(0x04)),
];

/// Look up a key by name: a letter, a digit, `F1`-`F12`, or one of the names
/// such as `ENTER`, `SPACE`, `UP`, `LSHIFT`. Case does not matter.
pub fn key_from_name(name: &str) -> Option<Key> {
    let upper = name.to_ascii_uppercase();
    if let [c] = upper.as_bytes() {
        return match c {
            b'A'..=b'Z' => Some(Key::Usage(c - b'A' + 4)),
            b'1'..=b'9' => Some(Key::Usage(c - b'1' + 30)),
            b'0' => Some(Key::Usage(39)),
            _ => None,
        };
    }
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
        if (1..=12).contains(&n) {
            return Some(Key::Usage(57 + n));
        }
    }
    NAMED.iter().find(|(n, _)| *n == upper).map(|(_, k)| *k)
}

impl crate::KeyboardState {
    /// Press a key. Returns false if six keys are already held.
    pub fn press(&mut self, key: Key) -> bool {
        match key {
            Key::Modifier(bit) => self.modifier |= bit,
            Key::Usage(code) => {
                if self.keys.contains(&code) {
                    return true;
                }
                match self.keys.iter_mut().find(|k| **k == 0) {
                    Some(slot) => *slot = code,
                    None => return false,
                }
            }
        }
        true
    }

    pub fn release(&mut self, key: Key) {
        match key {
            Key::Modifier(bit) => self.modifier &= !bit,
            Key::Usage(code) => {
                for k in self.keys.iter_mut().filter(|k| **k == code) {
                    *k = 0;
                }
            }
        }
    }
}

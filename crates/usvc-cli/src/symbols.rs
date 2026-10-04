// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Michael Wulff Nielsen
//! Symbol names from an objdump listing (`.lss`), for readable addresses.

#[derive(Default)]
pub struct Symbols {
    /// Sorted by address.
    entries: Vec<(u32, String)>,
}

impl Symbols {
    /// Collect the `0000abcd <name>:` label lines of a listing.
    pub fn from_lss(text: &[u8]) -> Symbols {
        let mut entries = Vec::new();
        for line in text.split(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(line);
            let line = line.trim_end();
            let Some((addr, rest)) = line.split_once(" <") else {
                continue;
            };
            let Some(name) = rest.strip_suffix(">:") else {
                continue;
            };
            if addr.len() == 8 {
                if let Ok(a) = u32::from_str_radix(addr, 16) {
                    entries.push((a, name.to_string()));
                }
            }
        }
        entries.sort();
        Symbols { entries }
    }

    /// `name+offset` for the symbol at or before `addr`.
    pub fn describe(&self, addr: u32) -> String {
        let idx = self.entries.partition_point(|(a, _)| *a <= addr);
        match idx.checked_sub(1).map(|i| &self.entries[i]) {
            Some((a, name)) if addr - a < 0x4000 => format!("{name}+{:#x}", addr - a),
            _ => "?".to_string(),
        }
    }
}

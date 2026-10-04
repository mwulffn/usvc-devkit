// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Michael Wulff Nielsen
//! Symbol names from an objdump listing (`.lss`), for readable addresses.

#[derive(Default)]
pub struct Symbols {
    /// Sorted by address.
    entries: Vec<(u32, String)>,
}

impl Symbols {
    /// Collect symbols from an objdump listing: the `0000abcd <name>:` labels
    /// of the disassembly and, if present (`objdump -t`), the functions in
    /// the symbol table. The table is what names code that runs from RAM.
    pub fn from_lss(text: &[u8]) -> Symbols {
        let mut entries = Vec::new();
        for line in text.split(|&b| b == b'\n') {
            let line = String::from_utf8_lossy(line);
            let line = line.trim_end();
            if let Some(entry) = parse_label(line).or_else(|| parse_table_function(line)) {
                entries.push(entry);
            }
        }
        entries.sort();
        entries.dedup();
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

fn parse_address(text: &str) -> Option<u32> {
    (text.len() == 8).then(|| u32::from_str_radix(text, 16).ok())?
}

/// `0000abcd <name>:`
fn parse_label(line: &str) -> Option<(u32, String)> {
    let (addr, rest) = line.split_once(" <")?;
    let name = rest.strip_suffix(">:")?;
    Some((parse_address(addr)?, name.to_string()))
}

/// A function line of `objdump -t`: `20000050 g     F .relocate\t00000b50 name`
fn parse_table_function(line: &str) -> Option<(u32, String)> {
    let (left, right) = line.split_once('\t')?;
    let mut words = left.split_whitespace();
    let addr = parse_address(words.next()?)?;
    if !words.any(|w| w == "F") {
        return None;
    }
    let name = right.split_whitespace().nth(1)?;
    Some((addr, name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_labels_and_table_functions() {
        let text = b"20000050 g     F .relocate\t00000b50 TCC1_Handler\n\
            20000700 g     O .bss\t00000010 videoData\n\
            00006100 <main>:\n    6100:\tb510 \tpush\t{r4, lr}\n";
        let syms = Symbols::from_lss(text);
        assert_eq!(syms.describe(0x2000_0056), "TCC1_Handler+0x6");
        assert_eq!(syms.describe(0x6102), "main+0x2");
        assert_eq!(syms.describe(0x2000_0704), "TCC1_Handler+0x6b4");
    }
}

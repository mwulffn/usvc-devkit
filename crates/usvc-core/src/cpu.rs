//! ARMv6-M (Cortex-M0+) Thumb interpreter with per-instruction cycle counts.
//!
//! Cycle counts follow the Cortex-M0+ documentation: 1 for data processing,
//! 2 for loads and stores (1 on the single-cycle I/O port), `1+N` for
//! multi-register transfers, 2 for taken branches, 3 for `BL`. Flash wait
//! states are not modelled.

use crate::{Fault, FaultKind, Machine};

pub const SP: usize = 13;
pub const LR: usize = 14;

/// Cycles from an interrupt being recognised to the first handler instruction.
const EXC_ENTRY_CYCLES: u64 = 15;
const EXC_RETURN_CYCLES: u64 = 13;
/// Cycles from the wake event to the instruction after `WFI`.
const WFI_WAKE_CYCLES: u64 = 2;

/// Priority value used for "no exception active" (thread mode).
const PRIO_THREAD: i32 = 256;

#[derive(Clone, Debug, Default)]
pub struct Cpu {
    pub r: [u32; 16],
    pub n: bool,
    pub z: bool,
    pub c: bool,
    pub v: bool,
    pub primask: bool,
    /// Number of the exception being handled, 0 in thread mode.
    pub ipsr: u32,
    /// Bit mask of active exception numbers.
    pub active: u64,
    pub vtor: u32,
    /// Latched NVIC pending bits for external interrupts.
    pub nvic_pending: u32,
    pub nvic_enable: u32,
    /// Level of each peripheral interrupt line.
    pub irq_level: u32,
    /// Interrupt priorities, 0..=3.
    pub irq_prio: [u8; 32],
    pub systick_prio: u8,
    pub systick_pending: bool,
}

impl Cpu {
    pub fn xpsr(&self) -> u32 {
        (self.n as u32) << 31
            | (self.z as u32) << 30
            | (self.c as u32) << 29
            | (self.v as u32) << 28
            | 1 << 24
            | self.ipsr
    }

    fn set_flags_from(&mut self, v: u32) {
        self.n = v >> 31 != 0;
        self.z = v >> 30 & 1 != 0;
        self.c = v >> 29 & 1 != 0;
        self.v = v >> 28 & 1 != 0;
    }

    fn exc_prio(&self, num: u32) -> i32 {
        match num {
            0 => PRIO_THREAD,
            2 => -2,
            3 => -1,
            15 => self.systick_prio as i32,
            n if n >= 16 => self.irq_prio[(n - 16) as usize & 31] as i32,
            _ => 0,
        }
    }

    /// Priority of the currently executing code, ignoring PRIMASK.
    fn current_prio(&self) -> i32 {
        let mut best = PRIO_THREAD;
        let mut act = self.active;
        while act != 0 {
            let n = act.trailing_zeros();
            act &= act - 1;
            best = best.min(self.exc_prio(n));
        }
        best
    }

    /// Highest-priority pending and enabled exception, as (number, priority).
    fn best_pending(&self) -> Option<(u32, i32)> {
        let mut best: Option<(u32, i32)> = None;
        if self.systick_pending {
            best = Some((15, self.systick_prio as i32));
        }
        let mut pend = self.nvic_pending & self.nvic_enable;
        while pend != 0 {
            let n = pend.trailing_zeros();
            pend &= pend - 1;
            let p = self.irq_prio[n as usize] as i32;
            if best.is_none_or(|(_, bp)| p < bp) {
                best = Some((16 + n, p));
            }
        }
        best
    }

    /// Record the level of the interrupt lines. A rising edge sets pending;
    /// a line still high when its handler returns pends again (see
    /// `exception_return`).
    pub fn set_irq_level(&mut self, level: u32) {
        self.nvic_pending |= level & !self.irq_level;
        self.irq_level = level;
    }

    /// Interrupt lines that are asserted and whose handler is not running.
    pub fn asserted_inactive(&self) -> u32 {
        self.irq_level & !((self.active >> 16) as u32)
    }
}

#[inline]
fn add_with_carry(a: u32, b: u32, carry: bool) -> (u32, bool, bool) {
    let wide = a as u64 + b as u64 + carry as u64;
    let res = wide as u32;
    let c = wide >> 32 != 0;
    let v = (!(a ^ b) & (a ^ res)) >> 31 != 0;
    (res, c, v)
}

impl Machine {
    #[inline]
    fn set_nz(&mut self, v: u32) {
        self.cpu.n = v >> 31 != 0;
        self.cpu.z = v == 0;
    }

    #[inline]
    fn add_flags(&mut self, a: u32, b: u32, carry: bool) -> u32 {
        let (res, c, v) = add_with_carry(a, b, carry);
        self.set_nz(res);
        self.cpu.c = c;
        self.cpu.v = v;
        res
    }

    #[inline]
    fn cond(&self, cond: u32) -> bool {
        let c = &self.cpu;
        let base = match cond >> 1 {
            0 => c.z,
            1 => c.c,
            2 => c.n,
            3 => c.v,
            4 => c.c && !c.z,
            5 => c.n == c.v,
            6 => c.n == c.v && !c.z,
            _ => true,
        };
        base != (cond & 1 != 0 && cond != 15)
    }

    /// Load cost: 1 cycle on the single-cycle I/O port, 2 elsewhere.
    #[inline]
    fn mem_cycles(addr: u32) -> u64 {
        if addr >> 28 == 6 {
            1
        } else {
            2
        }
    }

    pub(crate) fn fault(&mut self, kind: FaultKind, pc: u32, addr: u32) {
        if self.fault.is_none() {
            self.fault = Some(Fault {
                kind,
                pc,
                addr,
                cycle: self.cycles,
                lr: self.cpu.r[LR],
                sp: self.cpu.r[SP],
            });
        }
    }

    /// Take a pending interrupt if one can preempt the current code.
    #[inline]
    pub(crate) fn check_interrupts(&mut self) {
        let c = &self.cpu;
        if c.primask || ((c.nvic_pending & c.nvic_enable) == 0 && !c.systick_pending) {
            return;
        }
        if let Some((num, prio)) = self.cpu.best_pending() {
            if prio < self.cpu.current_prio() {
                self.enter_exception(num);
            }
        }
    }

    fn enter_exception(&mut self, num: u32) {
        let sp = self.cpu.r[SP];
        let align = sp & 4 != 0;
        let frame = sp.wrapping_sub(0x20) & !4;
        let xpsr = self.cpu.xpsr() | (align as u32) << 9;
        let pc = self.cpu.r[15];
        let regs = [
            self.cpu.r[0],
            self.cpu.r[1],
            self.cpu.r[2],
            self.cpu.r[3],
            self.cpu.r[12],
            self.cpu.r[LR],
            pc,
            xpsr,
        ];
        for (i, v) in regs.iter().enumerate() {
            self.write32(frame + 4 * i as u32, *v);
        }
        self.cpu.r[SP] = frame;
        self.stats.min_sp = self.stats.min_sp.min(frame);
        if self.cpu.ipsr == 0 {
            self.handler_enter = self.cycles;
        }
        self.cpu.r[LR] = if self.cpu.ipsr == 0 { 0xFFFF_FFF9 } else { 0xFFFF_FFF1 };
        self.cpu.ipsr = num;
        self.cpu.active |= 1 << num;
        if num == 15 {
            self.cpu.systick_pending = false;
        } else if num >= 16 {
            self.cpu.nvic_pending &= !(1 << (num - 16));
        }
        let vector = self.read32(self.cpu.vtor + 4 * num);
        self.cpu.r[15] = vector & !1;
        self.cycles += EXC_ENTRY_CYCLES;
        self.stats.exceptions += 1;
    }

    fn exception_return(&mut self) {
        let frame = self.cpu.r[SP];
        let mut regs = [0u32; 8];
        for (i, v) in regs.iter_mut().enumerate() {
            *v = self.read32(frame + 4 * i as u32);
        }
        self.cpu.r[0] = regs[0];
        self.cpu.r[1] = regs[1];
        self.cpu.r[2] = regs[2];
        self.cpu.r[3] = regs[3];
        self.cpu.r[12] = regs[4];
        self.cpu.r[LR] = regs[5];
        self.cpu.r[15] = regs[6] & !1;
        let xpsr = regs[7];
        self.cpu.r[SP] = (frame + 0x20) | (xpsr >> 9 & 1) << 2;
        self.cpu.active &= !(1u64 << self.cpu.ipsr);
        self.cpu.set_flags_from(xpsr);
        self.cpu.ipsr = xpsr & 0x3F;
        // A line that is still asserted pends its interrupt again.
        self.cpu.nvic_pending |= self.cpu.asserted_inactive();
        self.cycles += EXC_RETURN_CYCLES;
        if self.cpu.ipsr == 0 {
            self.stats.handler_cycles += self.cycles - self.handler_enter;
        }
    }

    /// Branch to `target`, handling exception return values in handler mode.
    #[inline]
    fn branch_to(&mut self, target: u32) {
        if target >= 0xFFFF_FFF0 && self.cpu.ipsr != 0 {
            self.exception_return();
        } else {
            self.cpu.r[15] = target & !1;
        }
    }

    fn wfi(&mut self) {
        let start = self.cycles;
        loop {
            if let Some((_, prio)) = self.cpu.best_pending() {
                if prio < self.cpu.current_prio() {
                    break;
                }
            }
            if self.next_event == u64::MAX {
                let pc = self.cpu.r[15];
                self.fault(FaultKind::Stuck, pc, 0);
                return;
            }
            self.cycles = self.cycles.max(self.next_event);
            self.process_events();
        }
        let waited = self.cycles - start;
        if self.cpu.ipsr != 0 {
            self.stats.wfi_count += 1;
            self.stats.wfi_min_slack = self.stats.wfi_min_slack.min(waited);
        }
        self.cycles += WFI_WAKE_CYCLES;
    }

    /// Execute one instruction.
    pub fn step(&mut self) {
        let pc = self.cpu.r[15];
        let op = self.fetch16(pc) as u32;
        let next = pc.wrapping_add(2);
        self.cpu.r[15] = next;
        let mut cyc: u64 = 1;

        macro_rules! reg {
            ($n:expr) => {
                self.cpu.r[($n) as usize]
            };
        }
        // Value of a register as an operand; PC reads as instruction + 4.
        macro_rules! rdreg {
            ($n:expr) => {{
                let n = ($n) as usize;
                if n == 15 {
                    pc.wrapping_add(4)
                } else {
                    self.cpu.r[n]
                }
            }};
        }

        match op >> 11 {
            // LSL / LSR / ASR immediate
            0b00000 => {
                let imm = op >> 6 & 31;
                let v = reg!(op >> 3 & 7);
                let res = if imm == 0 {
                    v
                } else {
                    self.cpu.c = v >> (32 - imm) & 1 != 0;
                    v << imm
                };
                reg!(op & 7) = res;
                self.set_nz(res);
            }
            0b00001 => {
                let imm = op >> 6 & 31;
                let v = reg!(op >> 3 & 7);
                let res = if imm == 0 {
                    self.cpu.c = v >> 31 != 0;
                    0
                } else {
                    self.cpu.c = v >> (imm - 1) & 1 != 0;
                    v >> imm
                };
                reg!(op & 7) = res;
                self.set_nz(res);
            }
            0b00010 => {
                let imm = op >> 6 & 31;
                let v = reg!(op >> 3 & 7) as i32;
                let res = if imm == 0 {
                    self.cpu.c = v < 0;
                    (v >> 31) as u32
                } else {
                    self.cpu.c = v >> (imm - 1) & 1 != 0;
                    (v >> imm) as u32
                };
                reg!(op & 7) = res;
                self.set_nz(res);
            }
            // ADD / SUB register or 3-bit immediate
            0b00011 => {
                let a = reg!(op >> 3 & 7);
                let b = if op & 0x400 != 0 { op >> 6 & 7 } else { reg!(op >> 6 & 7) };
                let res = if op & 0x200 != 0 {
                    self.add_flags(a, !b, true)
                } else {
                    self.add_flags(a, b, false)
                };
                reg!(op & 7) = res;
            }
            // MOV / CMP / ADD / SUB 8-bit immediate
            0b00100 => {
                let imm = op & 0xFF;
                reg!(op >> 8 & 7) = imm;
                self.set_nz(imm);
            }
            0b00101 => {
                let a = reg!(op >> 8 & 7);
                self.add_flags(a, !(op & 0xFF), true);
            }
            0b00110 => {
                let a = reg!(op >> 8 & 7);
                reg!(op >> 8 & 7) = self.add_flags(a, op & 0xFF, false);
            }
            0b00111 => {
                let a = reg!(op >> 8 & 7);
                reg!(op >> 8 & 7) = self.add_flags(a, !(op & 0xFF), true);
            }
            0b01000 => {
                if op & 0x400 == 0 {
                    cyc = self.data_processing(op);
                } else {
                    // Special data instructions and branch-exchange
                    let rm = op >> 3 & 15;
                    let rd = (op & 7) | (op >> 4 & 8);
                    match op >> 8 & 3 {
                        0 => {
                            let res = rdreg!(rd).wrapping_add(rdreg!(rm));
                            if rd == 15 {
                                self.cpu.r[15] = res & !1;
                                cyc = 2;
                            } else {
                                reg!(rd) = res;
                            }
                        }
                        1 => {
                            let (a, b) = (rdreg!(rd), rdreg!(rm));
                            self.add_flags(a, !b, true);
                        }
                        2 => {
                            let v = rdreg!(rm);
                            if rd == 15 {
                                self.cpu.r[15] = v & !1;
                                cyc = 2;
                            } else {
                                reg!(rd) = v;
                            }
                        }
                        _ => {
                            let target = rdreg!(rm);
                            if op & 0x80 != 0 {
                                self.cpu.r[LR] = next | 1;
                            }
                            cyc = 2;
                            self.branch_to(target);
                        }
                    }
                }
            }
            // LDR literal
            0b01001 => {
                let addr = (pc.wrapping_add(4) & !3).wrapping_add((op & 0xFF) << 2);
                reg!(op >> 8 & 7) = self.read32(addr);
                cyc = 2;
            }
            // Load / store with register offset
            0b01010 | 0b01011 => {
                let addr = reg!(op >> 3 & 7).wrapping_add(reg!(op >> 6 & 7));
                let rt = op & 7;
                cyc = Self::mem_cycles(addr);
                match op >> 9 & 7 {
                    0 => self.write32(addr, reg!(rt)),
                    1 => self.write16(addr, reg!(rt) as u16),
                    2 => self.write8(addr, reg!(rt) as u8),
                    3 => reg!(rt) = self.read8(addr) as i8 as i32 as u32,
                    4 => reg!(rt) = self.read32(addr),
                    5 => reg!(rt) = self.read16(addr) as u32,
                    6 => reg!(rt) = self.read8(addr) as u32,
                    _ => reg!(rt) = self.read16(addr) as i16 as i32 as u32,
                }
            }
            // STR / LDR word, 5-bit immediate
            0b01100 => {
                let addr = reg!(op >> 3 & 7).wrapping_add((op >> 6 & 31) << 2);
                cyc = Self::mem_cycles(addr);
                self.write32(addr, reg!(op & 7));
            }
            0b01101 => {
                let addr = reg!(op >> 3 & 7).wrapping_add((op >> 6 & 31) << 2);
                cyc = Self::mem_cycles(addr);
                reg!(op & 7) = self.read32(addr);
            }
            // STRB / LDRB
            0b01110 => {
                let addr = reg!(op >> 3 & 7).wrapping_add(op >> 6 & 31);
                cyc = Self::mem_cycles(addr);
                self.write8(addr, reg!(op & 7) as u8);
            }
            0b01111 => {
                let addr = reg!(op >> 3 & 7).wrapping_add(op >> 6 & 31);
                cyc = Self::mem_cycles(addr);
                reg!(op & 7) = self.read8(addr) as u32;
            }
            // STRH / LDRH
            0b10000 => {
                let addr = reg!(op >> 3 & 7).wrapping_add((op >> 6 & 31) << 1);
                cyc = Self::mem_cycles(addr);
                self.write16(addr, reg!(op & 7) as u16);
            }
            0b10001 => {
                let addr = reg!(op >> 3 & 7).wrapping_add((op >> 6 & 31) << 1);
                cyc = Self::mem_cycles(addr);
                reg!(op & 7) = self.read16(addr) as u32;
            }
            // STR / LDR SP-relative
            0b10010 => {
                let addr = reg!(SP).wrapping_add((op & 0xFF) << 2);
                self.write32(addr, reg!(op >> 8 & 7));
                cyc = 2;
            }
            0b10011 => {
                let addr = reg!(SP).wrapping_add((op & 0xFF) << 2);
                reg!(op >> 8 & 7) = self.read32(addr);
                cyc = 2;
            }
            // ADR, ADD rd, SP, imm
            0b10100 => {
                reg!(op >> 8 & 7) = (pc.wrapping_add(4) & !3).wrapping_add((op & 0xFF) << 2);
            }
            0b10101 => {
                reg!(op >> 8 & 7) = reg!(SP).wrapping_add((op & 0xFF) << 2);
            }
            0b10110 | 0b10111 => cyc = self.misc(op, pc),
            // STM / LDM
            0b11000 => {
                let rn = op >> 8 & 7;
                let mut addr = reg!(rn);
                let mut n = 0;
                for i in 0..8 {
                    if op >> i & 1 != 0 {
                        self.write32(addr, reg!(i));
                        addr = addr.wrapping_add(4);
                        n += 1;
                    }
                }
                reg!(rn) = addr;
                cyc = 1 + n;
            }
            0b11001 => {
                let rn = op >> 8 & 7;
                let mut addr = reg!(rn);
                let mut n = 0;
                for i in 0..8 {
                    if op >> i & 1 != 0 {
                        reg!(i) = self.read32(addr);
                        addr = addr.wrapping_add(4);
                        n += 1;
                    }
                }
                if op >> rn & 1 == 0 {
                    reg!(rn) = addr;
                }
                cyc = 1 + n;
            }
            // Conditional branch, UDF, SVC
            0b11010 | 0b11011 => {
                let cond = op >> 8 & 15;
                if cond == 14 {
                    cyc = self.udf(op, pc);
                } else if cond == 15 {
                    self.fault(FaultKind::Undefined, pc, op);
                } else if self.cond(cond) {
                    let off = ((op & 0xFF) as i8 as i32) << 1;
                    self.cpu.r[15] = pc.wrapping_add(4).wrapping_add(off as u32);
                    cyc = 2;
                }
            }
            // Unconditional branch
            0b11100 => {
                let off = ((op & 0x7FF) << 21) as i32 >> 20;
                self.cpu.r[15] = pc.wrapping_add(4).wrapping_add(off as u32);
                cyc = 2;
            }
            // 32-bit instructions
            0b11110 => cyc = self.wide(op, pc),
            _ => self.fault(FaultKind::Undefined, pc, op),
        }
        self.cycles += cyc;
        self.stats.instructions += 1;
    }

    fn data_processing(&mut self, op: u32) -> u64 {
        let rd = (op & 7) as usize;
        let a = self.cpu.r[rd];
        let b = self.cpu.r[(op >> 3 & 7) as usize];
        match op >> 6 & 15 {
            0 => {
                let r = a & b;
                self.cpu.r[rd] = r;
                self.set_nz(r);
            }
            1 => {
                let r = a ^ b;
                self.cpu.r[rd] = r;
                self.set_nz(r);
            }
            2 => {
                let s = b & 0xFF;
                let r = match s {
                    0 => a,
                    1..=31 => {
                        self.cpu.c = a >> (32 - s) & 1 != 0;
                        a << s
                    }
                    32 => {
                        self.cpu.c = a & 1 != 0;
                        0
                    }
                    _ => {
                        self.cpu.c = false;
                        0
                    }
                };
                self.cpu.r[rd] = r;
                self.set_nz(r);
            }
            3 => {
                let s = b & 0xFF;
                let r = match s {
                    0 => a,
                    1..=31 => {
                        self.cpu.c = a >> (s - 1) & 1 != 0;
                        a >> s
                    }
                    32 => {
                        self.cpu.c = a >> 31 != 0;
                        0
                    }
                    _ => {
                        self.cpu.c = false;
                        0
                    }
                };
                self.cpu.r[rd] = r;
                self.set_nz(r);
            }
            4 => {
                let s = b & 0xFF;
                let sa = a as i32;
                let r = match s {
                    0 => a,
                    1..=31 => {
                        self.cpu.c = sa >> (s - 1) & 1 != 0;
                        (sa >> s) as u32
                    }
                    _ => {
                        self.cpu.c = sa < 0;
                        (sa >> 31) as u32
                    }
                };
                self.cpu.r[rd] = r;
                self.set_nz(r);
            }
            5 => self.cpu.r[rd] = self.add_flags(a, b, self.cpu.c),
            6 => self.cpu.r[rd] = self.add_flags(a, !b, self.cpu.c),
            7 => {
                let s = b & 0xFF;
                let r = if s == 0 {
                    a
                } else {
                    let r = a.rotate_right(s & 31);
                    self.cpu.c = r >> 31 != 0;
                    r
                };
                self.cpu.r[rd] = r;
                self.set_nz(r);
            }
            8 => self.set_nz(a & b),
            9 => self.cpu.r[rd] = self.add_flags(!b, 0, true),
            10 => {
                self.add_flags(a, !b, true);
            }
            11 => {
                self.add_flags(a, b, false);
            }
            12 => {
                let r = a | b;
                self.cpu.r[rd] = r;
                self.set_nz(r);
            }
            13 => {
                let r = a.wrapping_mul(b);
                self.cpu.r[rd] = r;
                self.set_nz(r);
            }
            14 => {
                let r = a & !b;
                self.cpu.r[rd] = r;
                self.set_nz(r);
            }
            _ => {
                let r = !b;
                self.cpu.r[rd] = r;
                self.set_nz(r);
            }
        }
        1
    }

    /// Miscellaneous 16-bit instructions (`1011 xxxx`).
    fn misc(&mut self, op: u32, pc: u32) -> u64 {
        match op >> 8 & 15 {
            0 => {
                let imm = (op & 0x7F) << 2;
                let sp = self.cpu.r[SP];
                self.cpu.r[SP] = if op & 0x80 != 0 {
                    sp.wrapping_sub(imm)
                } else {
                    sp.wrapping_add(imm)
                };
                1
            }
            2 => {
                let v = self.cpu.r[(op >> 3 & 7) as usize];
                self.cpu.r[(op & 7) as usize] = match op >> 6 & 3 {
                    0 => v as i16 as i32 as u32,
                    1 => v as i8 as i32 as u32,
                    2 => v & 0xFFFF,
                    _ => v & 0xFF,
                };
                1
            }
            4 | 5 => {
                // PUSH
                let count = (op & 0x1FF).count_ones();
                let mut addr = self.cpu.r[SP].wrapping_sub(4 * count);
                self.cpu.r[SP] = addr;
                for i in 0..8 {
                    if op >> i & 1 != 0 {
                        self.write32(addr, self.cpu.r[i]);
                        addr = addr.wrapping_add(4);
                    }
                }
                if op & 0x100 != 0 {
                    self.write32(addr, self.cpu.r[LR]);
                }
                1 + count as u64
            }
            6 if op & 0xE0 == 0x60 => {
                // CPSIE / CPSID
                self.cpu.primask = op & 0x10 != 0;
                1
            }
            10 => {
                let v = self.cpu.r[(op >> 3 & 7) as usize];
                let r = match op >> 6 & 3 {
                    0 => v.swap_bytes(),
                    1 => (v >> 8 & 0x00FF_00FF) | (v << 8 & 0xFF00_FF00),
                    3 => (v as u16).swap_bytes() as i16 as i32 as u32,
                    _ => {
                        self.fault(FaultKind::Undefined, pc, op);
                        v
                    }
                };
                self.cpu.r[(op & 7) as usize] = r;
                1
            }
            12 | 13 => {
                // POP
                let mut addr = self.cpu.r[SP];
                let mut n = 0;
                for i in 0..8 {
                    if op >> i & 1 != 0 {
                        self.cpu.r[i] = self.read32(addr);
                        addr = addr.wrapping_add(4);
                        n += 1;
                    }
                }
                if op & 0x100 != 0 {
                    let target = self.read32(addr);
                    self.cpu.r[SP] = addr.wrapping_add(4);
                    self.branch_to(target);
                    3 + n
                } else {
                    self.cpu.r[SP] = addr;
                    1 + n
                }
            }
            14 => {
                self.fault(FaultKind::Breakpoint, pc, op & 0xFF);
                1
            }
            15 => {
                // Hints: NOP, YIELD, WFE, WFI, SEV
                if op & 0xFF == 0x30 {
                    self.cycles += 2;
                    self.wfi();
                    0
                } else {
                    1
                }
            }
            _ => {
                self.fault(FaultKind::Undefined, pc, op);
                1
            }
        }
    }

    /// 32-bit instructions: `BL`, `MSR`, `MRS` and the barriers.
    fn wide(&mut self, op: u32, pc: u32) -> u64 {
        let op2 = self.fetch16(pc.wrapping_add(2)) as u32;
        let next = pc.wrapping_add(4);
        self.cpu.r[15] = next;
        if op2 & 0xD000 == 0xD000 {
            // BL
            let s = op >> 10 & 1;
            let j1 = op2 >> 13 & 1;
            let j2 = op2 >> 11 & 1;
            let i1 = !(j1 ^ s) & 1;
            let i2 = !(j2 ^ s) & 1;
            let imm = s << 24 | i1 << 23 | i2 << 22 | (op & 0x3FF) << 12 | (op2 & 0x7FF) << 1;
            let off = ((imm << 7) as i32) >> 7;
            self.cpu.r[LR] = next | 1;
            self.cpu.r[15] = next.wrapping_add(off as u32);
            return 3;
        }
        if op & 0xFFF0 == 0xF380 && op2 & 0xFF00 == 0x8800 {
            // MSR
            let v = self.cpu.r[(op & 15) as usize];
            match op2 & 0xFF {
                0..=3 => self.cpu.set_flags_from(v),
                8 => self.cpu.r[SP] = v & !3,
                16 => self.cpu.primask = v & 1 != 0,
                _ => {}
            }
            return 3;
        }
        if op == 0xF3EF && op2 & 0xF000 == 0x8000 {
            // MRS
            let v = match op2 & 0xFF {
                0..=7 => self.cpu.xpsr() & 0xF000_003F,
                8 => self.cpu.r[SP],
                16 => self.cpu.primask as u32,
                _ => 0,
            };
            self.cpu.r[(op2 >> 8 & 15) as usize] = v;
            return 3;
        }
        if op == 0xF3BF && op2 & 0xFF00 == 0x8F00 {
            // DSB, DMB, ISB
            return 3;
        }
        self.fault(FaultKind::Undefined, pc, op << 16 | op2);
        1
    }
}

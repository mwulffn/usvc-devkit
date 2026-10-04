// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Michael Wulff Nielsen
//! Memory map and peripheral register dispatch.
//!
//! Peripheral registers live in a plain backing store so that anything
//! written reads back. A few peripherals are modelled on top of that; a few
//! status registers are forced to "ready" so that start-up code does not
//! wait forever.

use crate::{FaultKind, Machine, CPU_HZ, FLASH_SIZE, RAM_BASE, RAM_SIZE};

const APB_BASE: [u32; 3] = [0x4000_0000, 0x4100_0000, 0x4200_0000];

const PORT: u32 = 0x4100_4400;
const IOBUS: u32 = 0x6000_0000;
const TCC0: u32 = 0x4200_2000;
const TC3_CTRLA: u32 = 0x4200_2C00;
const TC4_COUNT: u32 = 0x4200_3010;
const EVSYS: u32 = 0x4200_0400;
const DAC_DATA: u32 = 0x4200_4808;
const SYSCTRL_PCLKSR: u32 = 0x4000_080C;
const SYSCTRL_DPLLSTATUS: u32 = 0x4000_0850;
const NVMCTRL_INTFLAG: u32 = 0x4100_4014;
/// Bytes written here appear as debug text on the host. The address is not
/// mapped on real hardware (a write there faults), so games must only use it
/// in emulator builds.
pub const DEBUG_PORT: u32 = 0x4200_5400;

const SCS: u32 = 0xE000_E000;
const SYST_CSR: u32 = 0xE000_E010;
const SYST_RVR: u32 = 0xE000_E014;
const SYST_CVR: u32 = 0xE000_E018;
const NVIC_ISER: u32 = 0xE000_E100;
const NVIC_ICER: u32 = 0xE000_E180;
const NVIC_ISPR: u32 = 0xE000_E200;
const NVIC_ICPR: u32 = 0xE000_E280;
const NVIC_IPR: u32 = 0xE000_E400;
const SCB_CPUID: u32 = 0xE000_ED00;
const SCB_ICSR: u32 = 0xE000_ED04;
const SCB_VTOR: u32 = 0xE000_ED08;
const SCB_AIRCR: u32 = 0xE000_ED0C;
const SCB_SHPR3: u32 = 0xE000_ED20;

#[inline]
fn size_mask(size: u32) -> u32 {
    match size {
        1 => 0xFF,
        2 => 0xFFFF,
        _ => 0xFFFF_FFFF,
    }
}

impl Machine {
    #[inline]
    pub(crate) fn fetch16(&mut self, addr: u32) -> u16 {
        let a = addr as usize;
        if a < FLASH_SIZE - 1 {
            return u16::from_le_bytes([self.flash[a], self.flash[a + 1]]);
        }
        let off = addr.wrapping_sub(RAM_BASE) as usize;
        if off < RAM_SIZE - 1 {
            return u16::from_le_bytes([self.ram[off], self.ram[off + 1]]);
        }
        self.fault(FaultKind::BusRead, addr, addr);
        0xBF00
    }

    #[inline]
    pub fn read32(&mut self, addr: u32) -> u32 {
        let off = addr.wrapping_sub(RAM_BASE) as usize;
        if off <= RAM_SIZE - 4 {
            return u32::from_le_bytes(self.ram[off..off + 4].try_into().unwrap());
        }
        let a = addr as usize;
        if a <= FLASH_SIZE - 4 {
            return u32::from_le_bytes(self.flash[a..a + 4].try_into().unwrap());
        }
        self.read_slow(addr, 4)
    }

    #[inline]
    pub fn read16(&mut self, addr: u32) -> u16 {
        let off = addr.wrapping_sub(RAM_BASE) as usize;
        if off <= RAM_SIZE - 2 {
            return u16::from_le_bytes([self.ram[off], self.ram[off + 1]]);
        }
        let a = addr as usize;
        if a <= FLASH_SIZE - 2 {
            return u16::from_le_bytes([self.flash[a], self.flash[a + 1]]);
        }
        self.read_slow(addr, 2) as u16
    }

    #[inline]
    pub fn read8(&mut self, addr: u32) -> u8 {
        let off = addr.wrapping_sub(RAM_BASE) as usize;
        if off < RAM_SIZE {
            return self.ram[off];
        }
        let a = addr as usize;
        if a < FLASH_SIZE {
            return self.flash[a];
        }
        self.read_slow(addr, 1) as u8
    }

    #[inline]
    pub fn write32(&mut self, addr: u32, v: u32) {
        let off = addr.wrapping_sub(RAM_BASE) as usize;
        if off <= RAM_SIZE - 4 {
            self.ram[off..off + 4].copy_from_slice(&v.to_le_bytes());
        } else {
            self.write_slow(addr, 4, v);
        }
    }

    #[inline]
    pub fn write16(&mut self, addr: u32, v: u16) {
        let off = addr.wrapping_sub(RAM_BASE) as usize;
        if off <= RAM_SIZE - 2 {
            self.ram[off..off + 2].copy_from_slice(&v.to_le_bytes());
        } else {
            self.write_slow(addr, 2, v as u32);
        }
    }

    #[inline]
    pub fn write8(&mut self, addr: u32, v: u8) {
        let off = addr.wrapping_sub(RAM_BASE) as usize;
        if off < RAM_SIZE {
            self.ram[off] = v;
        } else {
            self.write_slow(addr, 1, v as u32);
        }
    }

    /// Map an address to (APB bridge index, offset), folding the IOBUS alias
    /// of PORT onto the PORT registers.
    #[inline]
    fn apb_slot(&self, addr: u32) -> Option<(usize, usize)> {
        let addr = if addr & 0xFFFF_FE00 == IOBUS {
            PORT + (addr & 0x1FF)
        } else {
            addr
        };
        let bridge = (addr >> 24).wrapping_sub(0x40) as usize;
        if bridge < 3 {
            let off = (addr & 0x00FF_FFFF) as usize;
            if off < self.apb[bridge].len() {
                return Some((bridge, off));
            }
        }
        None
    }

    fn backing32(&self, bridge: usize, off: usize) -> u32 {
        let o = off & !3;
        u32::from_le_bytes(self.apb[bridge][o..o + 4].try_into().unwrap())
    }

    #[cold]
    fn read_slow(&mut self, addr: u32, size: u32) -> u32 {
        let shift = (addr & 3) * 8;
        let word = if addr >= SCS {
            self.scs_read(addr & !3)
        } else if let Some((bridge, off)) = self.apb_slot(addr) {
            let reg = APB_BASE[bridge] + (off as u32 & !3);
            self.periph_read(reg, bridge, off)
        } else if (0x0080_0000..0x0081_0000).contains(&addr) {
            // NVM user row and factory calibration area.
            0xFFFF_FFFF
        } else {
            let pc = self.cpu.r[15];
            self.fault(FaultKind::BusRead, pc, addr);
            0
        };
        word >> shift & size_mask(size)
    }

    #[cold]
    fn write_slow(&mut self, addr: u32, size: u32, v: u32) {
        let shift = (addr & 3) * 8;
        let mask = size_mask(size) << shift;
        let bits = v << shift & mask;
        if addr >= SCS {
            self.scs_write(addr & !3, bits, mask);
        } else if let Some((bridge, off)) = self.apb_slot(addr) {
            let o = off & !3;
            let old = self.backing32(bridge, off);
            let new = (old & !mask) | bits;
            self.apb[bridge][o..o + 4].copy_from_slice(&new.to_le_bytes());
            let reg = APB_BASE[bridge] + o as u32;
            self.periph_write(reg, new, bits, mask);
        } else {
            let pc = self.cpu.r[15];
            self.fault(FaultKind::BusWrite, pc, addr);
        }
    }

    fn periph_read(&mut self, reg: u32, bridge: usize, off: usize) -> u32 {
        if reg & !0x7F == PORT {
            return match reg - PORT {
                0x10..=0x20 => self.port_out,
                _ => self.backing32(bridge, off),
            };
        }
        if (TCC0..TCC0 + 0xC00).contains(&reg) {
            let n = ((reg - TCC0) / 0x400) as usize;
            return self.tcc_read(n, reg & 0x3FF);
        }
        match reg {
            r if r & !0xFF == EVSYS => self.evsys_read(r - EVSYS),
            TC4_COUNT => match self.tc_epoch {
                Some(start) => ((self.cycles - start) / (CPU_HZ / 1000)) as u32,
                None => 0,
            },
            SYSCTRL_PCLKSR => 0xFFFF_FFFF,
            SYSCTRL_DPLLSTATUS => 0x0F,
            NVMCTRL_INTFLAG => 1,
            _ => {
                self.unmodelled.insert(reg);
                self.backing32(bridge, off)
            }
        }
    }

    /// `new` is the full register value after the write; `bits` holds only
    /// the bytes written (for write-one-to-clear registers), `mask` says
    /// which bytes those were.
    fn periph_write(&mut self, reg: u32, new: u32, bits: u32, mask: u32) {
        if reg & !0x7F == PORT {
            let out = match reg - PORT {
                0x10 => (self.port_out & !mask) | bits,
                0x14 => self.port_out & !bits,
                0x18 => self.port_out | bits,
                0x1C => self.port_out ^ bits,
                _ => return,
            };
            self.port_out = out;
            if mask >> 16 != 0 {
                self.video.port_write(self.cycles, (out >> 16) as u16);
            }
            return;
        }
        if (TCC0..TCC0 + 0xC00).contains(&reg) {
            let n = ((reg - TCC0) / 0x400) as usize;
            self.tcc_write(n, reg & 0x3FF, new, bits);
            return;
        }
        match reg {
            r if r & !0xFF == EVSYS => self.evsys_write(r - EVSYS, new, bits),
            DAC_DATA => self.audio.push(((new & 0x3FF) as i16 - 512) * 64),
            TC3_CTRLA => {
                if new & 2 != 0 && self.tc_epoch.is_none() {
                    self.tc_epoch = Some(self.cycles);
                }
            }
            DEBUG_PORT => self.debug_out.push(bits as u8),
            _ => {
                self.unmodelled.insert(reg);
            }
        }
    }

    fn scs_read(&mut self, reg: u32) -> u32 {
        match reg {
            SYST_CSR => self.systick_read_ctrl(),
            SYST_RVR => self.systick.load,
            SYST_CVR => self.systick_value(),
            NVIC_ISER | NVIC_ICER => self.cpu.nvic_enable,
            NVIC_ISPR | NVIC_ICPR => self.cpu.nvic_pending,
            r if (NVIC_IPR..NVIC_IPR + 32).contains(&r) => {
                let i = (r - NVIC_IPR) as usize;
                (0..4).fold(0, |v, b| v | (self.cpu.irq_prio[i + b] as u32) << (6 + 8 * b))
            }
            SCB_CPUID => 0x410C_C601,
            SCB_ICSR => self.cpu.ipsr | (self.cpu.systick_pending as u32) << 26,
            SCB_VTOR => self.cpu.vtor,
            SCB_AIRCR => 0xFA05_0000,
            SCB_SHPR3 => (self.cpu.systick_prio as u32) << 30,
            _ => 0,
        }
    }

    fn scs_write(&mut self, reg: u32, bits: u32, mask: u32) {
        match reg {
            SYST_CSR => self.systick_write_ctrl(bits),
            SYST_RVR => {
                self.systick.load = bits & 0x00FF_FFFF;
                self.systick_reschedule();
            }
            SYST_CVR => {
                self.systick.base = self.cycles;
                self.systick.count_flag = false;
                self.systick_reschedule();
            }
            NVIC_ISER => self.cpu.nvic_enable |= bits,
            NVIC_ICER => self.cpu.nvic_enable &= !bits,
            NVIC_ISPR => self.cpu.nvic_pending |= bits,
            NVIC_ICPR => {
                self.cpu.nvic_pending &= !bits;
                self.cpu.nvic_pending |= self.cpu.asserted_inactive();
            }
            r if (NVIC_IPR..NVIC_IPR + 32).contains(&r) => {
                let i = (r - NVIC_IPR) as usize;
                for b in 0..4 {
                    if mask >> (8 * b) & 0xFF != 0 {
                        self.cpu.irq_prio[i + b] = (bits >> (6 + 8 * b) & 3) as u8;
                    }
                }
            }
            SCB_ICSR => {
                if bits & 1 << 26 != 0 {
                    self.cpu.systick_pending = true;
                }
                if bits & 1 << 25 != 0 {
                    self.cpu.systick_pending = false;
                }
            }
            SCB_VTOR => self.cpu.vtor = bits & 0xFFFF_FF80,
            SCB_AIRCR => {
                if bits & 4 != 0 {
                    let pc = self.cpu.r[15];
                    self.fault(FaultKind::ResetRequest, pc, 0);
                }
            }
            SCB_SHPR3 => self.cpu.systick_prio = (bits >> 30) as u8,
            _ => {}
        }
    }
}

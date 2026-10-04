// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Michael Wulff Nielsen
//! Host implementation of the bootloader library.
//!
//! Games built with `USE_BOOTLOADER` reach USB and FAT functions through a
//! table whose address is stored at `0x20B0`. We install a table that points
//! at `UDF` instructions; executing one performs the call here and returns
//! to the caller. This replaces the USB host controller and the SD card.

use std::collections::VecDeque;

use crate::cpu::LR;
use crate::{FaultKind, Machine};

/// Where the game looks for the pointer to the function table.
const TABLE_POINTER: u32 = 0x2000 + 44 * 4;
const TABLE: u32 = 0x2100;
const TRAPS: u32 = 0x2200;
const TRAP_COUNT: u32 = 64;
/// Trap that a guest callback returns to.
const CALLBACK_RETURN: u32 = TRAP_COUNT - 1;
/// Cycles charged for a library call.
const CALL_CYCLES: u64 = 20;

const FR_NOT_READY: u32 = 2;

/// Indices into the bootloader function table (`system.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum HleFn {
    PfMount = 0,
    PfOpen,
    PfRead,
    PfWrite,
    PfLseek,
    PfOpendir,
    PfReaddir,
    UsbHostInit,
    UsbHostTask,
    UsbPipe0Alloc,
    UsbPipeAlloc,
    UsbPipeFree,
    UsbCreateStandardRequest,
    UsbAddTransaction,
    UsbReleaseDevice,
    UsbGetState,
    UsbStringDescriptorToChar,
    UsbFindHidInterfaceAndEndpoint,
    KeyboardInstaller,
    KeyboardIsInstalled,
    KeyboardGetKey,
    KeyboardPoll,
    KeyboardGetState,
    KeyboardGetAscii,
    KeyboardGetAsciiEx,
    KeyboardSetInstallCallback,
    GamepadInstaller,
    GamepadIsInstalled,
    GamepadPoll,
    GamepadGetState,
    GamepadSetInstallCallback,
}

/// State of a USB boot keyboard: modifier bits and up to six HID usage codes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyboardState {
    pub modifier: u8,
    pub keys: [u8; 6],
}

/// Mirrors the kernel's `gamePadState_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GamepadState {
    pub buttons: u32,
    pub axes: [i16; 4],
    pub axis_min: i16,
    pub axis_max: i16,
    pub number_of_axes: u8,
    pub number_of_buttons: u8,
}

impl Default for GamepadState {
    fn default() -> Self {
        GamepadState {
            buttons: 0,
            axes: [127; 4],
            axis_min: 0,
            axis_max: 255,
            number_of_axes: 2,
            number_of_buttons: 12,
        }
    }
}

pub struct Hle {
    pub keyboard_present: bool,
    pub gamepad_present: bool,
    pub keyboard: KeyboardState,
    pub gamepad: GamepadState,
    /// Translated key presses waiting for `usbGetKey`.
    pub key_queue: VecDeque<u16>,
    pub calls: Vec<u64>,
    keyboard_callback: u32,
    gamepad_callback: u32,
    /// Guest callbacks still to run, and the caller to return to afterwards.
    pending_callbacks: Vec<u32>,
    resume_lr: u32,
}

impl Default for Hle {
    fn default() -> Self {
        Hle {
            keyboard_present: true,
            gamepad_present: false,
            keyboard: KeyboardState::default(),
            gamepad: GamepadState::default(),
            key_queue: VecDeque::new(),
            calls: vec![0; TRAP_COUNT as usize],
            keyboard_callback: 0,
            gamepad_callback: 0,
            pending_callbacks: Vec::new(),
            resume_lr: 0,
        }
    }
}

/// Port of the kernel's `OemToAscii`, without the locking keys.
pub fn oem_to_ascii(modifier: u8, key: u8) -> u16 {
    const NUM_KEYS: &[u8; 10] = b"!@#$%^&*()";
    const SYM_UP: &[u8; 12] = b"_+{}|~:\"~<>?";
    const SYM_LO: &[u8; 12] = b"-=[]\\ ;'`,./";
    const PAD: &[u8; 5] = b"/*-+\r";
    let shift = modifier & 0x22 != 0;
    let ctrl = modifier & 0x11 != 0;
    let ch = match key {
        4..=29 if ctrl => key - 3,
        4..=29 if shift => key - 4 + b'A',
        4..=29 => key - 4 + b'a',
        35 if ctrl => 0x1E,
        30..=39 if shift => NUM_KEYS[(key - 30) as usize],
        39 => b'0',
        30..=38 => key - 30 + b'1',
        45..=56 if ctrl => match key {
            45 => 0x1F,
            47 => 0x1B,
            48 => 0x1D,
            49 => 0x1C,
            _ => 0,
        },
        45..=56 if shift => SYM_UP[(key - 45) as usize],
        45..=56 => SYM_LO[(key - 45) as usize],
        84..=88 => PAD[(key - 84) as usize],
        44 => b' ',
        40 => b'\r',
        41 => 0x1B,
        42 => 0x08,
        76 => 0x7F,
        43 => 0x09,
        89..=97 => 0,
        _ => return (key as u16) << 8,
    };
    ch as u16
}

impl Machine {
    pub(crate) fn install_bootloader(&mut self) {
        let put32 = |flash: &mut [u8], addr: u32, v: u32| {
            flash[addr as usize..addr as usize + 4].copy_from_slice(&v.to_le_bytes());
        };
        put32(&mut self.flash, TABLE_POINTER, TABLE);
        for i in 0..TRAP_COUNT {
            let trap = TRAPS + 2 * i;
            put32(&mut self.flash, TABLE + 4 * i, trap | 1);
            let udf = 0xDE00u16 | i as u16;
            self.flash[trap as usize..trap as usize + 2].copy_from_slice(&udf.to_le_bytes());
        }
    }

    /// Say which input devices are plugged in.
    pub fn set_devices(&mut self, keyboard: bool, gamepad: bool) {
        self.hle.keyboard_present = keyboard;
        self.hle.gamepad_present = gamepad;
    }

    pub(crate) fn update_keyboard(&mut self, state: KeyboardState) {
        let old = self.hle.keyboard;
        for &k in state.keys.iter().filter(|&&k| k > 1) {
            if !old.keys.contains(&k) {
                self.hle.key_queue.push_back(oem_to_ascii(state.modifier, k));
            }
        }
        self.hle.keyboard = state;
    }

    /// `UDF` executed at `pc`: a library trap, or an undefined instruction.
    pub(crate) fn udf(&mut self, op: u32, pc: u32) -> u64 {
        if !(TRAPS..TRAPS + 2 * TRAP_COUNT).contains(&pc) {
            self.fault(FaultKind::Undefined, pc, op);
            return 1;
        }
        let index = (pc - TRAPS) / 2;
        self.hle.calls[index as usize] += 1;
        let mut ret = self.cpu.r[LR];
        if index == CALLBACK_RETURN {
            ret = self.hle.resume_lr;
        } else {
            self.cpu.r[0] = self.hle_call(index);
            if !self.hle.pending_callbacks.is_empty() {
                self.hle.resume_lr = ret;
            }
        }
        if let Some(callback) = self.hle.pending_callbacks.pop() {
            // Run the guest callback; it returns to the callback trap.
            self.cpu.r[LR] = (TRAPS + 2 * CALLBACK_RETURN) | 1;
            self.cpu.r[15] = callback & !1;
        } else {
            self.cpu.r[15] = ret & !1;
        }
        CALL_CYCLES
    }

    fn hle_call(&mut self, index: u32) -> u32 {
        let r0 = self.cpu.r[0];
        let r1 = self.cpu.r[1];
        const HOST_TASK: u32 = HleFn::UsbHostTask as u32;
        const KB_INSTALLED: u32 = HleFn::KeyboardIsInstalled as u32;
        const KB_GET_KEY: u32 = HleFn::KeyboardGetKey as u32;
        const KB_GET_STATE: u32 = HleFn::KeyboardGetState as u32;
        const KB_GET_ASCII: u32 = HleFn::KeyboardGetAscii as u32;
        const KB_GET_ASCII_EX: u32 = HleFn::KeyboardGetAsciiEx as u32;
        const KB_SET_CALLBACK: u32 = HleFn::KeyboardSetInstallCallback as u32;
        const GP_INSTALLED: u32 = HleFn::GamepadIsInstalled as u32;
        const GP_GET_STATE: u32 = HleFn::GamepadGetState as u32;
        const GP_SET_CALLBACK: u32 = HleFn::GamepadSetInstallCallback as u32;
        const PF_LAST: u32 = HleFn::PfReaddir as u32;
        match index {
            0..=PF_LAST => FR_NOT_READY,
            HOST_TASK => {
                // "Enumeration" completes on the first task call.
                let hle = &mut self.hle;
                for (present, cb) in [
                    (hle.gamepad_present, &mut hle.gamepad_callback),
                    (hle.keyboard_present, &mut hle.keyboard_callback),
                ] {
                    if present && *cb != 0 {
                        hle.pending_callbacks.push(*cb);
                        *cb = 0;
                    }
                }
                0
            }
            KB_INSTALLED => self.hle.keyboard_present as u32,
            KB_GET_KEY => match self.hle.key_queue.pop_front() {
                Some(k) => k as u32,
                None => -1i32 as u32,
            },
            KB_GET_STATE => {
                let kb = self.hle.keyboard;
                for (i, k) in kb.keys.iter().enumerate() {
                    self.write8(r0 + i as u32, *k);
                }
                self.write8(r1, kb.modifier);
                0
            }
            KB_GET_ASCII | KB_GET_ASCII_EX => {
                let kb = self.hle.keyboard;
                for (i, k) in kb.keys.iter().enumerate() {
                    let ch = oem_to_ascii(kb.modifier, *k);
                    if index == KB_GET_ASCII {
                        self.write8(r0 + i as u32, ch as u8);
                    } else {
                        self.write16(r0 + 2 * i as u32, ch);
                    }
                }
                0
            }
            KB_SET_CALLBACK => {
                self.hle.keyboard_callback = r0;
                0
            }
            GP_INSTALLED => self.hle.gamepad_present as u32,
            GP_GET_STATE => {
                if !self.hle.gamepad_present {
                    return 0;
                }
                let gp = self.hle.gamepad;
                self.write32(r0, gp.buttons);
                for (i, a) in gp.axes.iter().enumerate() {
                    self.write16(r0 + 4 + 2 * i as u32, *a as u16);
                }
                self.write16(r0 + 12, gp.axis_min as u16);
                self.write16(r0 + 14, gp.axis_max as u16);
                self.write8(r0 + 16, gp.number_of_axes);
                self.write8(r0 + 17, gp.number_of_buttons);
                1
            }
            GP_SET_CALLBACK => {
                self.hle.gamepad_callback = r0;
                0
            }
            _ => 0,
        }
    }
}

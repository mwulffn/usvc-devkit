// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Michael Wulff Nielsen
//! Portable emulator core for the uSVC console (ATSAMD21E18, Cortex-M0+).
//!
//! The core has no I/O and no platform dependencies. A front end loads a game
//! image, calls [`Machine::run_frame`], and reads the framebuffer and audio.

pub mod audio;
mod bus;
mod cpu;
mod hle;
pub mod keys;
mod timers;
pub mod usc;
mod video;

use std::collections::BTreeSet;

pub use cpu::Cpu;
pub use hle::{GamepadState, HleFn, KeyboardState};
pub use video::{SCREEN_HEIGHT, SCREEN_WIDTH};

use hle::Hle;
use timers::{Evsys, SysTick, Tcc};
use video::Video;

pub const FLASH_SIZE: usize = 0x4_0000;
pub const RAM_BASE: u32 = 0x2000_0000;
pub const RAM_SIZE: usize = 0x8000;
/// Address at which the game loader places a game.
pub const GAME_BASE: u32 = 0x6000;
pub const CPU_HZ: u64 = 48_000_000;
pub const CYCLES_PER_LINE: u64 = 1600;
pub const LINES_PER_FRAME: u64 = 525;
/// One DAC sample is written per scanline.
pub const AUDIO_HZ: u32 = (CPU_HZ / CYCLES_PER_LINE) as u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FaultKind {
    /// Read from an unmapped address.
    BusRead,
    /// Write to an unmapped or read-only address.
    BusWrite,
    /// Undefined instruction; `addr` holds the opcode.
    Undefined,
    /// `BKPT` executed; `addr` holds the immediate.
    Breakpoint,
    /// `WFI` with nothing that could ever wake it.
    Stuck,
    /// The program requested a system reset.
    ResetRequest,
}

/// Why the machine stopped. A faulted machine does not execute further.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fault {
    pub kind: FaultKind,
    pub pc: u32,
    pub addr: u32,
    pub cycle: u64,
    pub lr: u32,
    pub sp: u32,
}

/// Counters accumulated since the last [`Machine::take_stats`].
#[derive(Clone, Copy, Debug)]
pub struct Stats {
    pub instructions: u64,
    pub exceptions: u64,
    /// Cycles spent in handler mode (mostly the scanline interrupt).
    pub handler_cycles: u64,
    /// Number of `WFI` executed in handler mode (one per drawn line).
    pub wfi_count: u64,
    /// Smallest wait at such a `WFI`. Zero means the handler was late and the
    /// line is shifted.
    pub wfi_min_slack: u64,
    /// Lowest stack pointer seen at exception entry.
    pub min_sp: u32,
}

impl Default for Stats {
    fn default() -> Self {
        Stats {
            instructions: 0,
            exceptions: 0,
            handler_cycles: 0,
            wfi_count: 0,
            wfi_min_slack: u64::MAX,
            min_sp: u32::MAX,
        }
    }
}

/// Result of [`Machine::run_frame`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameResult {
    /// A vertical sync was reached.
    Frame,
    /// No vertical sync within two frame times (video not running yet).
    Timeout,
    Fault(Fault),
}

pub struct Machine {
    pub cpu: Cpu,
    /// CPU cycles since reset.
    pub cycles: u64,
    pub fault: Option<Fault>,
    pub stats: Stats,
    pub(crate) flash: Vec<u8>,
    pub(crate) ram: Vec<u8>,
    pub(crate) apb: [Vec<u8>; 3],
    pub(crate) next_event: u64,
    pub(crate) tcc: [Tcc; 3],
    pub(crate) evsys: Evsys,
    pub(crate) systick: SysTick,
    pub(crate) port_out: u32,
    /// Cycle at which the millisecond timer chain was started.
    pub(crate) tc_epoch: Option<u64>,
    pub(crate) video: Video,
    pub(crate) audio: Vec<i16>,
    pub(crate) frame_done: bool,
    pub(crate) hle: Hle,
    pub(crate) handler_enter: u64,
    /// Peripheral registers accessed that have no model behind them.
    pub(crate) unmodelled: BTreeSet<u32>,
    /// Text written by the game to the debug port.
    pub(crate) debug_out: Vec<u8>,
}

impl Default for Machine {
    fn default() -> Self {
        Self::new()
    }
}

impl Machine {
    pub fn new() -> Self {
        let mut m = Machine {
            cpu: Cpu::default(),
            cycles: 0,
            fault: None,
            stats: Stats::default(),
            flash: vec![0xFF; FLASH_SIZE],
            ram: vec![0; RAM_SIZE],
            apb: [vec![0; 0x2000], vec![0; 0x8000], vec![0; 0x5800]],
            next_event: u64::MAX,
            tcc: Default::default(),
            evsys: Evsys::default(),
            systick: SysTick::default(),
            port_out: 0,
            tc_epoch: None,
            video: Video::new(),
            audio: Vec::new(),
            frame_done: false,
            hle: Hle::default(),
            handler_enter: 0,
            unmodelled: BTreeSet::new(),
            debug_out: Vec::new(),
        };
        m.install_bootloader();
        m
    }

    /// Load a `.usc` package and start it the way the game loader does.
    pub fn load_usc(&mut self, data: &[u8]) -> Result<usc::Usc, usc::UscError> {
        let pkg = usc::Usc::parse(data)?;
        self.load_bin(&pkg.binary, GAME_BASE);
        Ok(pkg)
    }

    /// Copy a raw image into flash at `addr` and boot from its vector table.
    pub fn load_bin(&mut self, data: &[u8], addr: u32) {
        let start = addr as usize;
        let end = (start + data.len()).min(FLASH_SIZE);
        self.flash[start..end].copy_from_slice(&data[..end - start]);
        self.boot(addr);
    }

    /// Reset CPU state and jump to the image whose vector table is at `addr`.
    pub fn boot(&mut self, addr: u32) {
        self.cpu = Cpu::default();
        self.cpu.r[cpu::SP] = self.read32(addr);
        self.cpu.r[cpu::LR] = 0xFFFF_FFFF;
        self.cpu.r[15] = self.read32(addr + 4) & !1;
        self.fault = None;
    }

    /// Run until the next vertical sync.
    pub fn run_frame(&mut self) -> FrameResult {
        let limit = self.cycles + 2 * LINES_PER_FRAME * CYCLES_PER_LINE;
        self.frame_done = false;
        while !self.frame_done && self.cycles < limit && self.fault.is_none() {
            if self.cycles >= self.next_event {
                self.process_events();
            }
            self.check_interrupts();
            self.step();
        }
        match self.fault {
            Some(f) => FrameResult::Fault(f),
            None if self.frame_done => FrameResult::Frame,
            None => FrameResult::Timeout,
        }
    }

    /// Run for at least `n` cycles.
    pub fn run_cycles(&mut self, n: u64) -> Option<Fault> {
        let limit = self.cycles + n;
        while self.cycles < limit && self.fault.is_none() {
            if self.cycles >= self.next_event {
                self.process_events();
            }
            self.check_interrupts();
            self.step();
        }
        self.fault
    }

    /// The picture, `SCREEN_WIDTH` x `SCREEN_HEIGHT`, one `0x00RRGGBB` per pixel.
    pub fn framebuffer(&self) -> &[u32] {
        &self.video.fb
    }

    /// Audio samples produced since the last call, mono at [`AUDIO_HZ`].
    pub fn take_audio(&mut self) -> Vec<i16> {
        std::mem::take(&mut self.audio)
    }

    /// Text the game wrote to the debug port since the last call.
    pub fn take_debug_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.debug_out)
    }

    /// Counters since the last call.
    pub fn take_stats(&mut self) -> Stats {
        std::mem::take(&mut self.stats)
    }

    /// Set the keyboard state. Newly pressed keys are also queued for
    /// `usbGetKey`.
    pub fn set_keyboard(&mut self, state: KeyboardState) {
        self.update_keyboard(state);
    }

    /// Queue a key press for `usbGetKey` (ASCII or kernel key code).
    pub fn push_key(&mut self, key: u16) {
        self.hle.key_queue.push_back(key);
    }

    pub fn set_gamepad(&mut self, state: GamepadState) {
        self.hle.gamepad = state;
    }

    /// Number of calls made to each bootloader library function.
    pub fn hle_call_counts(&self) -> &[u64] {
        &self.hle.calls
    }

    /// Addresses of peripheral registers touched that are only stubbed.
    pub fn unmodelled_registers(&self) -> impl Iterator<Item = u32> + '_ {
        self.unmodelled.iter().copied()
    }

    /// Read guest memory without side effects on peripherals. Returns `None`
    /// outside flash and RAM.
    pub fn peek(&self, addr: u32, len: usize) -> Option<&[u8]> {
        let ram_off = addr.wrapping_sub(RAM_BASE) as usize;
        if ram_off.checked_add(len)? <= RAM_SIZE {
            return Some(&self.ram[ram_off..ram_off + len]);
        }
        let off = addr as usize;
        if off.checked_add(len)? <= FLASH_SIZE {
            return Some(&self.flash[off..off + len]);
        }
        None
    }

    pub fn peek32(&self, addr: u32) -> Option<u32> {
        self.peek(addr, 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Timing figures a front end can use to check the picture position.
    pub fn video_debug(&self) -> video::VideoDebug {
        self.video.debug
    }
}

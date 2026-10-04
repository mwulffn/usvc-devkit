//! TCC0-2, the event system and SysTick.
//!
//! Free-running timers are not ticked. Each keeps the cycle at which its
//! count was zero and the cycles of its next overflow and compare matches;
//! the machine runs the CPU until the earliest of those.

use crate::Machine;

const NEVER: u64 = u64::MAX;

const CTRLA: u32 = 0x00;
const SYNCBUSY: u32 = 0x08;
const EVCTRL: u32 = 0x20;
const INTENCLR: u32 = 0x24;
const INTENSET: u32 = 0x28;
const INTFLAG: u32 = 0x2C;
const COUNT: u32 = 0x34;
const PER: u32 = 0x40;
const CC0: u32 = 0x44;
const PERB: u32 = 0x6C;
const CCB0: u32 = 0x70;

const CTRLA_SWRST: u32 = 1;
const CTRLA_ENABLE: u32 = 2;
const EVCTRL_OVFEO: u32 = 1 << 8;
const EVCTRL_TCEI0: u32 = 1 << 14;
const EVCTRL_MCEO0: u32 = 1 << 24;
const INTFLAG_OVF: u32 = 1;
const INTFLAG_MC0: u32 = 1 << 16;

const EVACT_RETRIGGER: u32 = 1;
const EVACT_COUNTEV: u32 = 2;
const EVACT_INC: u32 = 4;

/// Event generator numbers of TCC0, TCC1, TCC2.
const GEN_OVF: [u8; 3] = [34, 41, 46];
const GEN_MC0: [u8; 3] = [37, 44, 49];
/// Event user numbers of the EV0 input of TCC0, TCC1, TCC2.
const USER_EV0: [usize; 3] = [0x04, 0x0A, 0x0E];

const IRQ_EVSYS: u32 = 8;
const IRQ_TCC0: u32 = 15;

const EVSYS_CHANNEL: u32 = 0x04;
const EVSYS_USER: u32 = 0x08;
const EVSYS_CHSTATUS: u32 = 0x0C;
const EVSYS_INTENCLR: u32 = 0x10;
const EVSYS_INTENSET: u32 = 0x14;
const EVSYS_INTFLAG: u32 = 0x18;
const PATH_ASYNC: u8 = 2;

/// Timer the horizontal sync comes from, and the one for vertical sync.
const HSYNC_TCC: usize = 1;
const VSYNC_TCC: usize = 0;

#[derive(Clone, Debug)]
pub struct Tcc {
    enabled: bool,
    evctrl: u32,
    intenset: u32,
    intflag: u32,
    per: u32,
    cc: [u32; 4],
    /// Count while stopped or counting events.
    count: u32,
    /// Cycle at which a free-running count was zero.
    base: u64,
    next_ovf: u64,
    next_mc: [u64; 4],
}

impl Default for Tcc {
    fn default() -> Self {
        Tcc {
            enabled: false,
            evctrl: 0,
            intenset: 0,
            intflag: 0,
            per: 0x00FF_FFFF,
            cc: [0; 4],
            count: 0,
            base: 0,
            next_ovf: NEVER,
            next_mc: [NEVER; 4],
        }
    }
}

impl Tcc {
    fn event_action(&self) -> u32 {
        if self.evctrl & EVCTRL_TCEI0 != 0 {
            self.evctrl & 7
        } else {
            0
        }
    }

    /// Counts input events instead of clock cycles.
    fn counts_events(&self) -> bool {
        matches!(self.event_action(), EVACT_COUNTEV | EVACT_INC)
    }

    fn period(&self) -> u64 {
        self.per as u64 + 1
    }

    fn reschedule(&mut self, now: u64) {
        self.next_ovf = NEVER;
        self.next_mc = [NEVER; 4];
        if !self.enabled || self.counts_events() {
            return;
        }
        let period = self.period();
        self.next_ovf = self.base + period;
        for k in 0..4 {
            // Only matches that raise an interrupt or an event are tracked.
            let wanted = (self.intenset | self.evctrl >> 8) & (INTFLAG_MC0 << k) != 0;
            if wanted && (self.cc[k] as u64) < period {
                let mut t = self.base + self.cc[k] as u64;
                if t <= now {
                    t += period;
                }
                self.next_mc[k] = t;
            }
        }
    }

    fn earliest(&self) -> u64 {
        self.next_mc.iter().fold(self.next_ovf, |a, &b| a.min(b))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Evsys {
    /// Event generator and path of each channel.
    chan_gen: [u8; 12],
    chan_path: [u8; 12],
    /// Channel + 1 feeding each user, 0 for none.
    user: [u8; 32],
    selected_user: u32,
    inten: u32,
    intflag: u32,
}

#[derive(Clone, Debug)]
pub struct SysTick {
    pub ctrl: u32,
    pub load: u32,
    /// Cycle at which the counter was last reloaded.
    pub base: u64,
    pub count_flag: bool,
    next: u64,
}

impl Default for SysTick {
    fn default() -> Self {
        SysTick {
            ctrl: 0,
            load: 0,
            base: 0,
            count_flag: false,
            next: NEVER,
        }
    }
}

impl Machine {
    pub(crate) fn tcc_read(&mut self, n: usize, off: u32) -> u32 {
        let now = self.cycles;
        let t = &self.tcc[n];
        match off {
            CTRLA => (t.enabled as u32) << 1,
            SYNCBUSY => 0,
            EVCTRL => t.evctrl,
            INTENCLR | INTENSET => t.intenset,
            INTFLAG => t.intflag,
            COUNT => {
                if t.enabled && !t.counts_events() {
                    ((now - t.base) % t.period()) as u32
                } else {
                    t.count
                }
            }
            PER | PERB => t.per,
            o if (CC0..CC0 + 16).contains(&o) => t.cc[((o - CC0) / 4) as usize],
            _ => 0,
        }
    }

    pub(crate) fn tcc_write(&mut self, n: usize, off: u32, new: u32, bits: u32) {
        let now = self.cycles;
        let t = &mut self.tcc[n];
        match off {
            CTRLA => {
                if new & CTRLA_SWRST != 0 {
                    *t = Tcc::default();
                } else {
                    let enable = new & CTRLA_ENABLE != 0;
                    if enable && !t.enabled {
                        t.base = now - t.count as u64;
                    } else if !enable && t.enabled && !t.counts_events() {
                        t.count = ((now - t.base) % t.period()) as u32;
                    }
                    t.enabled = enable;
                }
            }
            EVCTRL => t.evctrl = new,
            INTENCLR => t.intenset &= !bits,
            INTENSET => t.intenset |= bits,
            INTFLAG => t.intflag &= !bits,
            COUNT => {
                t.count = new;
                t.base = now - new as u64;
            }
            PER | PERB => t.per = new,
            o if (CC0..CC0 + 16).contains(&o) => t.cc[((o - CC0) / 4) as usize] = new,
            o if (CCB0..CCB0 + 16).contains(&o) => t.cc[((o - CCB0) / 4) as usize] = new,
            _ => return,
        }
        t.reschedule(now);
        self.update_irq();
        self.update_next_event();
    }

    pub(crate) fn evsys_read(&mut self, off: u32) -> u32 {
        match off {
            EVSYS_CHSTATUS => 0xFF,
            EVSYS_USER => {
                let u = self.evsys.selected_user;
                u | (self.evsys.user[u as usize] as u32) << 8
            }
            EVSYS_INTENCLR | EVSYS_INTENSET => self.evsys.inten,
            EVSYS_INTFLAG => self.evsys.intflag,
            _ => 0,
        }
    }

    pub(crate) fn evsys_write(&mut self, off: u32, new: u32, bits: u32) {
        let e = &mut self.evsys;
        match off {
            EVSYS_CHANNEL => {
                let ch = (new & 15) as usize;
                if ch < 12 {
                    e.chan_gen[ch] = (new >> 16 & 0x7F) as u8;
                    e.chan_path[ch] = (new >> 24 & 3) as u8;
                }
            }
            EVSYS_USER => {
                e.selected_user = new & 31;
                e.user[(new & 31) as usize] = (new >> 8 & 31) as u8;
            }
            EVSYS_INTENCLR => e.inten &= !bits,
            EVSYS_INTENSET => e.inten |= bits,
            EVSYS_INTFLAG => e.intflag &= !bits,
            _ => return,
        }
        self.update_irq();
    }

    /// Recompute the interrupt lines from the peripheral flags.
    pub(crate) fn update_irq(&mut self) {
        let mut level = 0;
        for (i, t) in self.tcc.iter().enumerate() {
            if t.intflag & t.intenset != 0 {
                level |= 1 << (IRQ_TCC0 + i as u32);
            }
        }
        if self.evsys.intflag & self.evsys.inten != 0 {
            level |= 1 << IRQ_EVSYS;
        }
        self.cpu.set_irq_level(level);
    }

    pub(crate) fn update_next_event(&mut self) {
        let timers = self.tcc.iter().fold(NEVER, |a, t| a.min(t.earliest()));
        self.next_event = timers.min(self.systick.next);
    }

    /// Handle every timer event due at or before the current cycle, in order.
    pub(crate) fn process_events(&mut self) {
        let now = self.cycles;
        loop {
            let mut when = self.systick.next;
            let mut source = None;
            for (i, t) in self.tcc.iter().enumerate() {
                if t.earliest() < when {
                    when = t.earliest();
                    source = Some(i);
                }
            }
            if when > now {
                break;
            }
            match source {
                Some(i) => self.tcc_timer_event(i, when),
                None => self.systick_event(),
            }
        }
        self.update_irq();
        self.update_next_event();
    }

    fn tcc_timer_event(&mut self, i: usize, when: u64) {
        let t = &mut self.tcc[i];
        let period = t.period();
        if t.next_ovf == when {
            t.base += period;
            t.next_ovf += period;
            self.tcc_overflow(i, when);
            return;
        }
        for k in 0..4 {
            if t.next_mc[k] == when {
                t.next_mc[k] += period;
                t.intflag |= INTFLAG_MC0 << k;
                if t.evctrl & (EVCTRL_MCEO0 << k) != 0 {
                    self.fire_event(GEN_MC0[i] + k as u8, when);
                }
                return;
            }
        }
    }

    fn tcc_overflow(&mut self, i: usize, when: u64) {
        self.tcc[i].intflag |= INTFLAG_OVF;
        if i == HSYNC_TCC {
            self.video.hsync(when);
        }
        if i == VSYNC_TCC {
            self.video.vsync();
            self.frame_done = true;
        }
        if self.tcc[i].evctrl & EVCTRL_OVFEO != 0 {
            self.fire_event(GEN_OVF[i], when);
        }
    }

    /// An event generator fired: flag its channels and drive their users.
    fn fire_event(&mut self, generator: u8, when: u64) {
        for ch in 0..12 {
            if self.evsys.chan_gen[ch] != generator {
                continue;
            }
            if self.evsys.chan_path[ch] != PATH_ASYNC && ch < 8 {
                self.evsys.intflag |= 1 << (8 + ch);
            }
            for (n, user) in USER_EV0.iter().enumerate() {
                if self.evsys.user[*user] as usize == ch + 1 {
                    self.tcc_input_event(n, when);
                }
            }
        }
    }

    fn tcc_input_event(&mut self, i: usize, when: u64) {
        let t = &mut self.tcc[i];
        if !t.enabled {
            return;
        }
        match t.event_action() {
            EVACT_RETRIGGER => {
                t.base = when;
                t.reschedule(when);
            }
            EVACT_COUNTEV | EVACT_INC => {
                t.count += 1;
                if t.count > t.per {
                    t.count = 0;
                    self.tcc_overflow(i, when);
                }
            }
            _ => {}
        }
    }

    fn systick_event(&mut self) {
        let s = &mut self.systick;
        let period = s.load as u64 + 1;
        s.base += period;
        s.next += period;
        s.count_flag = true;
        if s.ctrl & 2 != 0 {
            self.cpu.systick_pending = true;
        }
    }

    pub(crate) fn systick_reschedule(&mut self) {
        let s = &mut self.systick;
        s.next = if s.ctrl & 1 != 0 && s.load != 0 {
            let period = s.load as u64 + 1;
            let elapsed = self.cycles.saturating_sub(s.base);
            s.base += elapsed / period * period;
            s.base + period
        } else {
            NEVER
        };
        self.update_next_event();
    }

    pub(crate) fn systick_value(&self) -> u32 {
        let s = &self.systick;
        if s.ctrl & 1 == 0 || s.load == 0 {
            return 0;
        }
        let elapsed = self.cycles.saturating_sub(s.base) % (s.load as u64 + 1);
        s.load - elapsed as u32
    }

    pub(crate) fn systick_read_ctrl(&mut self) -> u32 {
        let flag = std::mem::take(&mut self.systick.count_flag);
        self.systick.ctrl & 7 | (flag as u32) << 16
    }

    pub(crate) fn systick_write_ctrl(&mut self, v: u32) {
        let was_enabled = self.systick.ctrl & 1 != 0;
        self.systick.ctrl = v & 7;
        if !was_enabled && v & 1 != 0 {
            self.systick.base = self.cycles;
        }
        self.systick_reschedule();
    }
}

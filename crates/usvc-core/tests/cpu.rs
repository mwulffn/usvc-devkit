//! Instruction, cycle-count and interrupt behaviour on hand-assembled code.

use usvc_core::{FaultKind, Machine, GAME_BASE};

const CODE: u32 = GAME_BASE + 0x100;
const HANDLER: u32 = GAME_BASE + 0x200;
const STACK_TOP: u32 = 0x2000_8000;

/// Build a machine with `main` at `CODE` and every vector pointing at
/// `handler`, placed at `HANDLER`.
fn machine(main: &[u16], handler: &[u16]) -> Machine {
    let mut image = vec![0u8; 0x300];
    let mut put32 = |off: usize, v: u32| image[off..off + 4].copy_from_slice(&v.to_le_bytes());
    put32(0, STACK_TOP);
    put32(4, CODE | 1);
    for vector in 2..48 {
        put32(vector * 4, HANDLER | 1);
    }
    for (base, code) in [(0x100, main), (0x200, handler)] {
        for (i, op) in code.iter().enumerate() {
            image[base + 2 * i..base + 2 * i + 2].copy_from_slice(&op.to_le_bytes());
        }
    }
    let mut m = Machine::new();
    m.load_bin(&image, GAME_BASE);
    m.cpu.vtor = GAME_BASE;
    m
}

const B_SELF: u16 = 0xE7FE;
const BX_LR: u16 = 0x4770;

#[test]
fn add_sets_carry_overflow_and_zero() {
    // MOVS r0,#1; LSLS r0,r0,#31; ADDS r0,r0,r0
    let mut m = machine(&[0x2001, 0x07C0, 0x1800, B_SELF], &[]);
    m.step();
    m.step();
    assert_eq!(m.cpu.r[0], 0x8000_0000);
    assert!(m.cpu.n && !m.cpu.z);
    m.step();
    assert_eq!(m.cpu.r[0], 0);
    assert!(m.cpu.z && m.cpu.c && m.cpu.v && !m.cpu.n);
}

#[test]
fn subtract_and_compare_set_borrow() {
    // MOVS r0,#3; MOVS r1,#5; SUBS r2,r0,r1; CMP r1,#5
    let mut m = machine(&[0x2003, 0x2105, 0x1A42, 0x2905, B_SELF], &[]);
    for _ in 0..3 {
        m.step();
    }
    assert_eq!(m.cpu.r[2], 0xFFFF_FFFE);
    assert!(m.cpu.n && !m.cpu.c);
    m.step();
    assert!(m.cpu.z && m.cpu.c);
}

#[test]
fn cycle_counts_match_cortex_m0plus() {
    // MOVS r0,#7 (1); PUSH {r0,lr} (3); POP {r1} (2); LDR r2,[pc,#4] (2);
    // MULS r0,r1 (1); B +0 (2)
    let mut m = machine(&[0x2007, 0xB501, 0xBC02, 0x4A01, 0x4348, 0xE7FF, B_SELF], &[]);
    let mut costs = Vec::new();
    for _ in 0..6 {
        let before = m.cycles;
        m.step();
        costs.push(m.cycles - before);
    }
    assert_eq!(costs, [1, 3, 2, 2, 1, 2]);
    assert_eq!(m.cpu.r[1], 7);
    assert_eq!(m.cpu.r[0], 49);
}

#[test]
fn io_port_store_takes_one_cycle() {
    // LDR r1,[pc,#4]; STR r0,[r1]; B .; (pad); .word 0x60000010
    let mut m = machine(&[0x4901, 0x6008, B_SELF, 0, 0x0010, 0x6000], &[]);
    m.step();
    assert_eq!(m.cpu.r[1], 0x6000_0010);
    let before = m.cycles;
    m.step();
    assert_eq!(m.cycles - before, 1);
}

#[test]
fn bl_and_return() {
    // BL +4 (to the MOVS r0,#9); B .; MOVS r0,#9; BX LR
    let mut m = machine(&[0xF000, 0xF801, B_SELF, 0x2009, BX_LR], &[]);
    m.step();
    assert_eq!(m.cpu.r[15], CODE + 6);
    assert_eq!(m.cpu.r[14], (CODE + 4) | 1);
    m.step();
    m.step();
    assert_eq!(m.cpu.r[15], CODE + 4);
    assert_eq!(m.cpu.r[0], 9);
}

#[test]
fn systick_interrupt_enters_and_returns() {
    // main: B .   handler: ADDS r4,#1; BX LR
    let mut m = machine(&[B_SELF], &[0x3401, BX_LR]);
    m.write32(0xE000_E014, 99);
    m.write32(0xE000_E010, 7);
    m.run_cycles(1000);
    assert!(m.fault.is_none());
    assert!((8..=10).contains(&m.cpu.r[4]), "ticks: {}", m.cpu.r[4]);
    assert_eq!(m.cpu.r[13], STACK_TOP);
}

/// Pend interrupt 16 (priority 1), whose handler masks interrupts and waits.
fn wfi_machine(also_pend: u32) -> Machine {
    // handler: CPSID i; WFI; MOVS r5,#1; B .
    let mut m = machine(&[B_SELF], &[0xB672, 0xBF30, 0x2501, B_SELF]);
    m.write32(0xE000_E40C, 0xC000_0000); // interrupt 15: priority 3
    m.write32(0xE000_E410, 0x0000_0040); // interrupt 16: priority 1
    m.write32(0xE000_E100, 1 << 16 | 1 << 15 | 1 << 8);
    m.write32(0xE000_E200, 1 << 16 | also_pend);
    m
}

#[test]
fn wfi_ignores_lower_priority_interrupt() {
    let mut m = wfi_machine(1 << 15);
    m.run_cycles(200);
    assert_eq!(m.fault.map(|f| f.kind), Some(FaultKind::Stuck));
    assert_eq!(m.cpu.r[5], 0);
}

#[test]
fn wfi_wakes_on_masked_higher_priority_interrupt_without_taking_it() {
    let mut m = wfi_machine(0);
    // SysTick (priority 0) becomes pending while the handler waits.
    m.write32(0xE000_E014, 99);
    m.write32(0xE000_E010, 3);
    m.run_cycles(200);
    assert!(m.fault.is_none());
    assert_eq!(m.cpu.r[5], 1);
    assert_eq!(m.cpu.ipsr, 32, "still in the first handler");
}

#[test]
fn undefined_instruction_faults() {
    let mut m = machine(&[0xDE42], &[]);
    m.step();
    let f = m.fault.expect("fault");
    assert_eq!(f.kind, FaultKind::Undefined);
    assert_eq!(f.pc, CODE);
}

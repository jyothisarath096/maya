#![allow(dead_code)]

use x86_64::{
    instructions::port::Port,
    structures::{
        idt::InterruptStackFrame,
        paging::{Page, PageTableFlags, PhysFrame, Size4KiB},
    },
    PhysAddr, VirtAddr,
};

use crate::memory::vmm;
use crate::sched::queue;
use crate::{serial_print, serial_print_usize};

const APIC_BASE: u64 = 0xFEE00000;
const APIC_EOI: u32 = 0x00B0;
const APIC_TIMER_LVT: u32 = 0x0320;
const APIC_TIMER_INIT: u32 = 0x0380;
const APIC_TIMER_CURRENT: u32 = 0x0390;
const APIC_TIMER_DIV: u32 = 0x03E0;
const TIMER_VECTOR: u8 = 0x20;
const APIC_TIMER_COUNT: u32 = 625_000;
static mut BSP_TICK_COUNT: u64 = 0;

pub fn init() {
    let apic_page = Page::<Size4KiB>::containing_address(VirtAddr::new(APIC_BASE));
    let apic_frame = PhysFrame::from_start_address(PhysAddr::new(APIC_BASE))
        .expect("APIC base not page aligned");
    let _ = vmm::map_page(
        apic_page,
        apic_frame,
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE,
    );

    unsafe {
        write_apic(APIC_TIMER_DIV, 0x3);
        write_apic(APIC_TIMER_INIT, u32::MAX);

        let mut port61 = Port::<u8>::new(0x61);
        let val = port61.read();
        port61.write((val & 0xFD) | 0x01);

        let mut cmd = Port::<u8>::new(0x43);
        cmd.write(0xB2);

        let mut data = Port::<u8>::new(0x42);
        data.write(0xFF);
        data.write(0xFF);

        let val = port61.read() & 0xFE;
        port61.write(val);
        port61.write(val | 0x01);

        let apic_start = read_apic(APIC_TIMER_CURRENT);

        let mut status_port = Port::<u8>::new(0x61);
        loop {
            if status_port.read() & 0x20 != 0 {
                break;
            }
        }

        let apic_end = read_apic(APIC_TIMER_CURRENT);
        let ticks_per_10ms = apic_start.saturating_sub(apic_end);

        write_apic(APIC_TIMER_DIV, 0x3);
        write_apic(APIC_TIMER_LVT, 0x20000 | TIMER_VECTOR as u32);
        write_apic(APIC_TIMER_INIT, ticks_per_10ms);
    }
}

pub fn init_local_apic_timer() {
    init();
}

pub fn init_ap_timer(core_id: u8) {
    let apic_page = Page::<Size4KiB>::containing_address(VirtAddr::new(APIC_BASE));
    let apic_frame = match PhysFrame::from_start_address(PhysAddr::new(APIC_BASE)) {
        Ok(frame) => frame,
        Err(_) => return,
    };
    let _ = vmm::map_page(
        apic_page,
        apic_frame,
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_CACHE,
    );

    unsafe {
        write_apic(APIC_TIMER_DIV, 0x3);
        write_apic(APIC_TIMER_INIT, APIC_TIMER_COUNT);
        write_apic(APIC_TIMER_LVT, 0x0002_0000 | TIMER_VECTOR as u32);
    }

    serial_print("Timer: core ");
    serial_print_usize(core_id as usize);
    serial_print(" initialised\n");
}

pub fn rdtsc() -> u64 {
    unsafe {
        let lo: u32;
        let hi: u32;
        core::arch::asm!(
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
            options(nomem, nostack, preserves_flags)
        );
        ((hi as u64) << 32) | lo as u64
    }
}

pub extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    timer_tick_handler();
}

pub fn timer_tick_handler() {
    let core_id = get_current_core_id();
    if core_id == 0 {
        unsafe {
            BSP_TICK_COUNT = BSP_TICK_COUNT.saturating_add(1);
        }
        queue::tick(queue::current_pid());
        if queue::tick_count().is_multiple_of(100) {
            crate::sched::balance::rebalance();
        }
    }
    queue::tick_core(core_id);
    unsafe {
        let apic_eoi = (APIC_BASE + APIC_EOI as u64) as *mut u32;
        apic_eoi.write_volatile(0);
    }
}

pub fn get_current_core_id() -> u8 {
    unsafe {
        let ptr = 0xFEE00020u64 as *const u32;
        ((ptr.read_volatile() >> 24) & 0xFF) as u8
    }
}

pub fn current_tick() -> u64 {
    unsafe { BSP_TICK_COUNT }
}

unsafe fn read_apic(offset: u32) -> u32 {
    let ptr = (APIC_BASE + offset as u64) as *const u32;
    unsafe { ptr.read_volatile() }
}

unsafe fn write_apic(offset: u32, val: u32) {
    let ptr = (APIC_BASE + offset as u64) as *mut u32;
    unsafe { ptr.write_volatile(val) }
}

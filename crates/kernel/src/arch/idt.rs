use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::sched::timer::timer_handler;
use x86_64::structures::idt::InterruptDescriptorTable;

use super::{gdt, interrupts};

struct IdtState {
    idt: UnsafeCell<MaybeUninit<InterruptDescriptorTable>>,
    initialized: AtomicBool,
}

unsafe impl Sync for IdtState {}

static IDT_STATE: IdtState = IdtState {
    idt: UnsafeCell::new(MaybeUninit::uninit()),
    initialized: AtomicBool::new(false),
};

pub fn init() {
    unsafe {
        if !IDT_STATE.initialized.load(Ordering::Acquire) {
            let mut idt = InterruptDescriptorTable::new();
            idt.divide_error
                .set_handler_fn(interrupts::divide_error_handler);
            idt.breakpoint
                .set_handler_fn(interrupts::breakpoint_handler);
            idt.invalid_opcode
                .set_handler_fn(interrupts::invalid_opcode_handler);
            idt.double_fault
                .set_handler_fn(interrupts::double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
            idt.general_protection_fault
                .set_handler_fn(interrupts::general_protection_fault_handler);
            idt.page_fault
                .set_handler_fn(interrupts::page_fault_handler);
            idt[0x20].set_handler_fn(timer_handler);

            (*IDT_STATE.idt.get()).write(idt);
            IDT_STATE.initialized.store(true, Ordering::Release);
        }

        (*IDT_STATE.idt.get()).assume_init_ref().load();
    }
}

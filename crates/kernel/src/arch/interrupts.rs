use crate::{halt_loop, serial_print};
use x86_64::{
    registers::control::Cr2,
    structures::idt::{InterruptStackFrame, PageFaultErrorCode},
};

pub extern "x86-interrupt" fn divide_error_handler(_frame: InterruptStackFrame) {
    serial_print("EXCEPTION: DIVIDE ERROR\n");
    halt_loop();
}

pub extern "x86-interrupt" fn breakpoint_handler(_frame: InterruptStackFrame) {
    if crate::proc::process_is_done() {
        crate::proc::restore_to_main();
    }
    serial_print("EXCEPTION: BREAKPOINT\n");
}

pub extern "x86-interrupt" fn invalid_opcode_handler(_frame: InterruptStackFrame) {
    serial_print("EXCEPTION: INVALID OPCODE\n");
    halt_loop();
}

pub extern "x86-interrupt" fn double_fault_handler(
    _frame: InterruptStackFrame,
    _error: u64,
) -> ! {
    serial_print("EXCEPTION: DOUBLE FAULT\n");
    halt_loop();
}

pub extern "x86-interrupt" fn general_protection_fault_handler(
    _frame: InterruptStackFrame,
    error_code: u64,
) {
    serial_print("EXCEPTION: GENERAL PROTECTION FAULT error=0x");
    serial_print_hex(error_code);
    serial_print("\n");
    halt_loop();
}

pub extern "x86-interrupt" fn page_fault_handler(
    _frame: InterruptStackFrame,
    _error_code: PageFaultErrorCode,
) {
    match Cr2::read() {
        Ok(address) => {
            serial_print("EXCEPTION: PAGE FAULT cr2=0x");
            serial_print_hex(address.as_u64());
            serial_print("\n");
        }
        Err(_) => {
            serial_print("EXCEPTION: PAGE FAULT cr2=<invalid>\n");
        }
    }
    halt_loop();
}

fn serial_print_hex(value: u64) {
    for shift in (0..16).rev() {
        let digit = ((value >> (shift * 4)) & 0xF) as u8;
        let byte = match digit {
            0..=9 => b'0' + digit,
            _ => b'A' + (digit - 10),
        };
        let bytes = [byte];
        if let Ok(text) = core::str::from_utf8(&bytes) {
            serial_print(text);
        }
    }
}

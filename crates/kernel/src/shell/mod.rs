#![allow(dead_code)]
#![allow(static_mut_refs)]

pub mod commands;
pub mod intent;
pub mod query;
pub mod response;

use crate::{context, fb_print, serial_print};

const INPUT_BUF_SIZE: usize = 256;
static mut INPUT_BUFFER: [u8; INPUT_BUF_SIZE] = [0u8; INPUT_BUF_SIZE];

pub fn init() {
    context::init();
    serial_print("Shell initialised\n");
    fb_print("Shell initialised\n");
}

pub fn ask(user_query: &str) {
    let system_prompt = query::build_system_prompt();
    let full_query = query::build_query(&system_prompt, user_query);
    serial_print(&full_query);
    fb_print(&full_query);
    let response = response::read_response_blocking();
    serial_print("Maya: ");
    fb_print("Maya: ");
    serial_print(response);
    fb_print(response);
    serial_print("\n");
    fb_print("\n");
    context::set(user_query, response, Some(1000));
}

pub fn run_interactive() -> ! {
    print_prompt();
    let mut pos = 0usize;

    loop {
        let lsr: u8;
        unsafe {
            core::arch::asm!(
                "in al, dx",
                in("dx") 0x3FDu16,
                out("al") lsr,
                options(nomem, nostack, preserves_flags)
            );
        }
        if lsr & 0x01 == 0 {
            unsafe {
                core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
            }
            continue;
        }

        let byte: u8;
        unsafe {
            core::arch::asm!(
                "in al, dx",
                in("dx") 0x3F8u16,
                out("al") byte,
                options(nomem, nostack, preserves_flags)
            );
        }

        match byte {
            b'\r' | b'\n' => {
                serial_print("\n");
                fb_print("\n");
                if pos > 0 {
                    let input = unsafe { core::str::from_utf8(&INPUT_BUFFER[..pos]).unwrap_or("") };
                    handle_input(input);
                    pos = 0;
                    print_prompt();
                }
            }
            b'\x08' | b'\x7F' => {
                if pos > 0 {
                    pos -= 1;
                    serial_print("\x08 \x08");
                    fb_print("\x08 \x08");
                }
            }
            32..=126 => {
                if pos < INPUT_BUF_SIZE - 1 {
                    unsafe {
                        INPUT_BUFFER[pos] = byte;
                    }
                    pos += 1;
                    let ch = [byte];
                    if let Ok(s) = core::str::from_utf8(&ch) {
                        serial_print(s);
                        fb_print(s);
                    }
                }
            }
            _ => {}
        }
    }
}

fn handle_input(input: &str) {
    let parsed = commands::parse(input);

    match &parsed {
        commands::Intent::AskAI { query } => {
            ask(query);
        }
        commands::Intent::ExplainDecision { context } => {
            let q = alloc::format!("Explain this Maya OS decision: {}", context);
            ask(&q);
        }
        _ => {
            let response = intent::execute(parsed);
            serial_print("Maya: ");
            fb_print("Maya: ");
            serial_print(&response);
            fb_print(&response);
            serial_print("\n");
            fb_print("\n");
        }
    }
}

fn print_prompt() {
    serial_print("\nmaya> ");
    fb_print("\nmaya> ");
}

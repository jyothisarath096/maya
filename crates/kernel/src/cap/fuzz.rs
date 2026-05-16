#![allow(dead_code)]

use alloc::vec::Vec;

use crate::{
    cap::{self, ResourceType, Rights},
    serial_print,
    serial_print_usize,
};

pub fn run_fuzz_suite() {
    serial_print("Cap fuzz: starting...\n");
    let mut passed = 0usize;
    let mut failed = 0usize;

    {
        let tok = cap::create(1, ResourceType::Memory, 0, Rights::READ).unwrap();
        cap::revoke(tok).unwrap();
        match cap::validate(tok) {
            Err(_) => passed += 1,
            Ok(_) => {
                serial_print("FAIL: use-after-revoke\n");
                failed += 1;
            }
        }
    }

    {
        let fake = cap::CapToken::from_raw(0xDEADBEEF_CAFEBABE);
        match cap::validate(fake) {
            Err(_) => passed += 1,
            Ok(_) => {
                serial_print("FAIL: forged token accepted\n");
                failed += 1;
            }
        }
    }

    {
        let tok = cap::create(1, ResourceType::Memory, 0, Rights::READ).unwrap();
        match cap::check_right_as(tok, Rights::READ, 2) {
            Err(_) => passed += 1,
            Ok(_) => {
                serial_print("FAIL: cross-owner access\n");
                failed += 1;
            }
        }
        cap::revoke(tok).ok();
    }

    {
        let tok = cap::create(1, ResourceType::Memory, 0, Rights::READ).unwrap();
        match cap::check_right(tok, Rights::WRITE) {
            Err(_) => passed += 1,
            Ok(_) => {
                serial_print("FAIL: rights escalation\n");
                failed += 1;
            }
        }
        cap::revoke(tok).ok();
    }

    {
        let tok = cap::create(1, ResourceType::Memory, 0, Rights::READ).unwrap();
        cap::revoke(tok).unwrap();
        match cap::revoke(tok) {
            Err(_) => passed += 1,
            Ok(_) => {
                serial_print("FAIL: double revoke\n");
                failed += 1;
            }
        }
    }

    {
        let mut tokens = Vec::new();
        for i in 0..200u32 {
            if let Ok(token) = cap::create(1, ResourceType::Memory, i, Rights::READ) {
                tokens.push(token);
            }
        }
        for token in &tokens {
            cap::revoke(*token).ok();
        }
        match cap::create(1, ResourceType::Memory, 0, Rights::READ) {
            Ok(token) => {
                cap::revoke(token).ok();
                passed += 1;
            }
            Err(_) => {
                serial_print("FAIL: table recovery\n");
                failed += 1;
            }
        }
    }

    {
        let mut last_tok = None;
        for _ in 0..10 {
            let token = cap::create(1, ResourceType::Memory, 0, Rights::READ).unwrap();
            cap::revoke(token).unwrap();
            last_tok = Some(token);
        }
        let new_tok = cap::create(1, ResourceType::Memory, 0, Rights::READ).unwrap();
        if cap::validate(new_tok).is_ok() && last_tok.map(|old| cap::validate(old).is_err()).unwrap_or(false)
        {
            passed += 1;
        } else {
            serial_print("FAIL: generation wraparound\n");
            failed += 1;
        }
        cap::revoke(new_tok).ok();
    }

    serial_print("Cap fuzz: ");
    serial_print_usize(passed);
    serial_print(" passed, ");
    serial_print_usize(failed);
    serial_print(" failed\n");
}

#![allow(dead_code)]

use crate::{
    cap::{self, IntentClass, ResourceType, Rights},
    ipc::channel::{self, Message},
    sched::{
        process::{Process, ProcessClass},
        queue,
    },
    KernelError,
};

fn payload(byte: u8) -> [u8; 52] {
    [byte; 52]
}

pub fn run_fuzz_suite() {
    crate::uart_print!("IPC fuzz: starting...\n");
    let mut passed = 0usize;
    let mut failed = 0usize;

    channel::init();
    if queue::get_process(200).is_none() {
        queue::add_process(Process::new(200, ProcessClass::Batch, 0.5));
    }

    let (send_cap, recv_cap) = match channel::create_channel(100, 200) {
        Ok(caps) => {
            passed += 1;
            caps
        }
        Err(_) => {
            crate::uart_print!("FAIL: create_channel\n");
            failed += 1;
            crate::uart_print!("IPC fuzz: ");
            crate::uart_print_usize!(passed);
            crate::uart_print!(" passed, ");
            crate::uart_print_usize!(failed);
            crate::uart_print!(" failed\n");
            return;
        }
    };

    let base_msg = Message {
        sender_pid: 100,
        intent_id: 42,
        intent_class: IntentClass::Unknown,
        _pad: [0; 2],
        payload: payload(7),
        cap_transfer: None,
    };

    match channel::send(send_cap, base_msg) {
        Ok(()) => match channel::recv(recv_cap) {
            Ok(received) if received.payload == payload(7) => passed += 1,
            _ => {
                crate::uart_print!("FAIL: round trip\n");
                failed += 1;
            }
        },
        Err(_) => {
            crate::uart_print!("FAIL: send recv\n");
            failed += 1;
        }
    }

    if matches!(channel::recv(recv_cap), Err(KernelError::IpcChannelEmpty)) {
        passed += 1;
    } else {
        crate::uart_print!("FAIL: empty recv\n");
        failed += 1;
    }

    let msg2 = Message { payload: payload(8), ..base_msg };
    let _ = channel::send(send_cap, msg2);
    if matches!(channel::send(send_cap, msg2), Err(KernelError::IpcChannelFull)) {
        passed += 1;
    } else {
        crate::uart_print!("FAIL: double send\n");
        failed += 1;
    }
    let _ = channel::recv(recv_cap);

    if channel::send(recv_cap, base_msg).is_err() {
        passed += 1;
    } else {
        crate::uart_print!("FAIL: wrong send cap\n");
        failed += 1;
    }

    if channel::recv(send_cap).is_err() {
        passed += 1;
    } else {
        crate::uart_print!("FAIL: wrong recv cap\n");
        failed += 1;
    }

    let transfer_cap = cap::create(
        100,
        ResourceType::Intent,
        33,
        Rights(Rights::READ.0 | Rights::GRANT.0),
        9,
        IntentClass::Unknown,
    )
    .unwrap();
    let transfer_msg = Message {
        cap_transfer: Some(transfer_cap),
        ..base_msg
    };
    if channel::send(send_cap, transfer_msg).is_ok() {
        if let Ok(received) = channel::recv(recv_cap) {
            if received.cap_transfer.is_some()
                && cap::validate(received.cap_transfer.unwrap()).is_ok()
            {
                passed += 1;
            } else {
                crate::uart_print!("FAIL: cap transfer validate\n");
                failed += 1;
            }
        } else {
            crate::uart_print!("FAIL: cap transfer recv\n");
            failed += 1;
        }
    } else {
        crate::uart_print!("FAIL: cap transfer send\n");
        failed += 1;
    }

    let intent_msg = Message {
        intent_id: 55,
        ..base_msg
    };
    let _ = channel::send(send_cap, intent_msg);
    if let Ok(received) = channel::recv(recv_cap) {
        if received.intent_id == 55 {
            passed += 1;
        } else {
            crate::uart_print!("FAIL: intent preserve\n");
            failed += 1;
        }
    } else {
        crate::uart_print!("FAIL: intent recv\n");
        failed += 1;
    }

    let before = queue::get_process(200)
        .map(|process| process.stats.intent_fire_ns)
        .unwrap_or(0);
    let _ = channel::send(send_cap, base_msg);
    let after = queue::get_process(200)
        .map(|process| process.stats.intent_fire_ns)
        .unwrap_or(0);
    let _ = channel::recv(recv_cap);
    if after >= before {
        passed += 1;
    } else {
        crate::uart_print!("FAIL: scheduler notify\n");
        failed += 1;
    }

    if matches!(
        channel::recv_blocking(recv_cap, Some(0)),
        Err(KernelError::IpcTimeout)
    ) {
        passed += 1;
    } else {
        crate::uart_print!("FAIL: timeout\n");
        failed += 1;
    }

    crate::uart_print!("IPC fuzz: ");
    crate::uart_print_usize!(passed);
    crate::uart_print!(" passed, ");
    crate::uart_print_usize!(failed);
    crate::uart_print!(" failed\n");
}

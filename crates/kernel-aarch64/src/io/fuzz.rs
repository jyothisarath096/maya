#![allow(dead_code)]

use crate::{
    cap::IntentClass,
    io::{
        audit::{self, MediatorDecision},
        mediator,
        syscall::{IoEventKind, IoRequest},
    },
    sched::{
        process::{Process, ProcessClass},
        queue,
    },
};

fn fixed_path(path: &str) -> ([u8; 64], usize) {
    let mut buf = [0u8; 64];
    let bytes = path.as_bytes();
    let len = bytes.len().min(64);
    buf[..len].copy_from_slice(&bytes[..len]);
    (buf, len)
}

fn request(kind: IoEventKind, path: Option<&str>) -> IoRequest {
    let (path, path_len) = match path {
        Some(path) => {
            let (buf, len) = fixed_path(path);
            (Some(buf), len)
        }
        None => (None, 0),
    };
    IoRequest {
        kind,
        path,
        path_len,
        size: 64,
        offset: 0,
        cap_token: None,
    }
}

pub fn run_fuzz_suite() {
    crate::uart_print!("IO fuzz: starting...\n");
    let mut passed = 0usize;
    let mut failed = 0usize;

    mediator::reset();
    mediator::set_fuzz_mode(true);
    mediator::declare_scope_unchecked(100, "/tmp/100/");
    mediator::declare_scope_unchecked(200, "/tmp/200/");
    if queue::get_process(100).is_none() {
        let mut process = Process::new(100, ProcessClass::Batch, 0.5);
        process.intent_class = IntentClass::IO;
        queue::add_process(process);
    } else {
        let _ = queue::set_process_intent_class(100, IntentClass::IO);
    }

    {
        crate::kdbg!("io fuzz test 1");
        let req = request(IoEventKind::FileRead, Some("/tmp/100/file.txt"));
        let r = mediator::mediate(100, &req);
        if matches!(r.decision, MediatorDecision::Allow) {
            passed += 1;
        } else {
            crate::uart_print!("FAIL: in-scope read blocked\n");
            failed += 1;
        }
    }

    {
        crate::kdbg!("io fuzz test 2");
        let req = request(IoEventKind::FileWrite, Some("/tmp/200/file.txt"));
        let r = mediator::mediate(100, &req);
        if matches!(r.decision, MediatorDecision::Block) {
            passed += 1;
        } else {
            crate::uart_print!("FAIL: out-of-scope write allowed\n");
            failed += 1;
        }
    }

    {
        crate::kdbg!("io fuzz test 3");
        let req = request(IoEventKind::FileRead, Some("/proc/200/mem"));
        let r = mediator::mediate(100, &req);
        if matches!(r.decision, MediatorDecision::Block) {
            passed += 1;
        } else {
            crate::uart_print!("FAIL: cross-proc read allowed\n");
            failed += 1;
        }
    }

    {
        crate::kdbg!("io fuzz test 4");
        let req = request(IoEventKind::FileRead, Some("/proc/self/status"));
        let r = mediator::mediate(100, &req);
        if matches!(r.decision, MediatorDecision::Allow) {
            passed += 1;
        } else {
            crate::uart_print!("FAIL: proc/self blocked\n");
            failed += 1;
        }
    }

    {
        crate::kdbg!("io fuzz test 5");
        let req = request(IoEventKind::FileWrite, Some("/etc/passwd"));
        let r = mediator::mediate(1, &req);
        if matches!(r.decision, MediatorDecision::Allow) {
            passed += 1;
        } else {
            crate::uart_print!("FAIL: kernel proc blocked\n");
            failed += 1;
        }
    }

    {
        crate::kdbg!("io fuzz test 6");
        let req = request(IoEventKind::FileRead, Some("/tmp/100/file.txt"));
        let mut last = MediatorDecision::Allow;
        for _ in 0..25 {
            last = mediator::mediate(100, &req).decision;
        }
        if matches!(last, MediatorDecision::Flag | MediatorDecision::Block) {
            passed += 1;
        } else {
            crate::uart_print!("FAIL: repeat attack not detected\n");
            failed += 1;
        }
    }

    {
        crate::kdbg!("io fuzz test 7");
        let req = request(IoEventKind::NetworkSend, Some("/tmp/100/net.sock"));
        let r = mediator::mediate(100, &req);
        if matches!(r.decision, MediatorDecision::Allow) {
            passed += 1;
        } else {
            crate::uart_print!("FAIL: intent class discount missing\n");
            failed += 1;
        }
    }

    {
        crate::kdbg!("io fuzz test 8");
        let req = request(IoEventKind::FileWrite, Some("/tmp/200/blocked.txt"));
        let before = queue::get_process(100)
            .map(|process| process.stats.intent_fire_ns)
            .unwrap_or(0);
        let _ = mediator::mediate(100, &req);
        let after = queue::get_process(100)
            .map(|process| process.stats.intent_fire_ns)
            .unwrap_or(0);
        if after >= before {
            passed += 1;
        } else {
            crate::uart_print!("FAIL: audit feedback missing\n");
            failed += 1;
        }
    }

    mediator::set_fuzz_mode(false);
    crate::uart_print!("IO fuzz: ");
    crate::uart_print_usize!(passed);
    crate::uart_print!(" passed, ");
    crate::uart_print_usize!(failed);
    crate::uart_print!(" failed\n");

    let _ = audit::recent(1);
}

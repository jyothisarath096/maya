#![allow(dead_code)]

use crate::{
    io::{
        audit::{IoEventKind, MediatorDecision},
        mediator,
        syscall::IoRequest,
    },
    serial_print,
    serial_print_usize,
};

pub fn run_fuzz_suite() {
    serial_print("IO fuzz: starting...\n");
    let mut passed = 0usize;
    let mut failed = 0usize;

    mediator::declare_scope(100, "/tmp/100/");
    mediator::declare_scope(200, "/tmp/200/");

    {
        let req = IoRequest {
            kind: IoEventKind::FileRead,
            path: Some("/tmp/100/file.txt".into()),
            size: 64,
            offset: 0,
        };
        let r = mediator::mediate(100, &req);
        if matches!(r.decision, MediatorDecision::Allow) {
            passed += 1;
        } else {
            serial_print("FAIL: in-scope read blocked\n");
            failed += 1;
        }
    }

    {
        let req = IoRequest {
            kind: IoEventKind::FileWrite,
            path: Some("/tmp/200/file.txt".into()),
            size: 64,
            offset: 0,
        };
        let r = mediator::mediate(100, &req);
        if matches!(r.decision, MediatorDecision::Block) {
            passed += 1;
        } else {
            serial_print("FAIL: out-of-scope write allowed\n");
            failed += 1;
        }
    }

    {
        let req = IoRequest {
            kind: IoEventKind::FileRead,
            path: Some("/proc/200/mem".into()),
            size: 64,
            offset: 0,
        };
        let r = mediator::mediate(100, &req);
        if matches!(r.decision, MediatorDecision::Block) {
            passed += 1;
        } else {
            serial_print("FAIL: cross-proc read allowed\n");
            failed += 1;
        }
    }

    {
        let req = IoRequest {
            kind: IoEventKind::FileRead,
            path: Some("/proc/self/status".into()),
            size: 64,
            offset: 0,
        };
        let r = mediator::mediate(100, &req);
        if matches!(r.decision, MediatorDecision::Allow) {
            passed += 1;
        } else {
            serial_print("FAIL: proc/self blocked\n");
            failed += 1;
        }
    }

    {
        let req = IoRequest {
            kind: IoEventKind::FileWrite,
            path: Some("/etc/passwd".into()),
            size: 64,
            offset: 0,
        };
        let r = mediator::mediate(1, &req);
        if matches!(r.decision, MediatorDecision::Allow) {
            passed += 1;
        } else {
            serial_print("FAIL: kernel proc blocked\n");
            failed += 1;
        }
    }

    {
        let req = IoRequest {
            kind: IoEventKind::FileRead,
            path: Some("/tmp/100/file.txt".into()),
            size: 64,
            offset: 0,
        };
        let mut last = MediatorDecision::Allow;
        for _ in 0..25 {
            last = mediator::mediate(100, &req).decision;
        }
        if matches!(last, MediatorDecision::Flag | MediatorDecision::Block) {
            passed += 1;
        } else {
            serial_print("FAIL: repeat attack not detected\n");
            failed += 1;
        }
    }

    serial_print("IO fuzz: ");
    serial_print_usize(passed);
    serial_print(" passed, ");
    serial_print_usize(failed);
    serial_print(" failed\n");
}

#![allow(dead_code)]

use super::counters::rdtsc;
use crate::{serial_print, serial_print_usize};

pub fn run() {
    serial_print("=== Maya Performance Benchmarks ===\n");

    {
        let (tx, rx) = crate::ipc::create_channel().unwrap();
        let start = rdtsc();
        for _ in 0..10000u32 {
            let msg = crate::ipc::Message {
                sender_pid: 0,
                payload: [0u8; 56],
                cap_transfer: None,
            };
            crate::ipc::send(tx, msg).ok();
            crate::ipc::recv(rx).ok();
        }
        let end = rdtsc();
        let cycles_per_rtt = (end - start) / 10000;
        serial_print("IPC round-trip: ");
        serial_print_usize(cycles_per_rtt as usize);
        serial_print(" cycles\n");
    }

    {
        let start = rdtsc();
        for i in 0..10000u32 {
            let tok = crate::cap::create(1, crate::cap::ResourceType::Memory, i, crate::cap::Rights::READ)
                .unwrap();
            crate::cap::validate(tok).unwrap();
            crate::cap::revoke(tok).unwrap();
        }
        let end = rdtsc();
        let cycles = (end - start) / 10000;
        serial_print("Cap create+validate+revoke: ");
        serial_print_usize(cycles as usize);
        serial_print(" cycles\n");
    }

    {
        use crate::io::audit::IoEventKind;
        use crate::io::mediator;
        use crate::io::syscall::IoRequest;

        mediator::declare_scope(99, "/tmp/99/");
        // Use only 20 iterations to avoid repeat-request
        // anomaly detection (triggers after ~25 identical requests)
        let req = IoRequest {
            kind: IoEventKind::FileRead,
            path: Some("/tmp/99/bench.txt".into()),
            size: 64,
            offset: 0,
        };
        let start = rdtsc();
        for _ in 0..20u32 {
            mediator::mediate(99, &req);
        }
        let end = rdtsc();
        let cycles = (end - start) / 20;
        serial_print("IO mediation decision: ");
        serial_print_usize(cycles as usize);
        serial_print(" cycles\n");
    }

    {
        use crate::sched::policy;
        use crate::sched::process::{Process, ProcessClass};

        let proc = Process::new(99, ProcessClass::Interactive, 0.5);
        let start = rdtsc();
        for _ in 0..100u32 {
            policy::score(&proc, 1000, 65536, 256);
        }
        let end = rdtsc();
        let cycles = (end - start) / 100;
        serial_print("AI scheduler score: ");
        serial_print_usize(cycles as usize);
        serial_print(" cycles\n");
    }

    {
        let start = rdtsc();
        for _ in 0..1000u32 {
            let frame = crate::memory::pmm::alloc_frame().unwrap();
            crate::memory::pmm::free_frame(frame).unwrap();
        }
        let end = rdtsc();
        let cycles = (end - start) / 1000;
        serial_print("PMM alloc+free: ");
        serial_print_usize(cycles as usize);
        serial_print(" cycles\n");
    }

    serial_print("=== Benchmarks complete ===\n");
}

pub fn run_benchmarks() {
    run();
}

pub fn run_smp_benchmarks() {
    let online_cores = crate::sched::queue::online_core_count();
    serial_print("=== SMP Benchmarks ===\n");
    serial_print("Online cores: ");
    serial_print_usize(online_cores);
    serial_print("\n");

    for core_id in 0..16u8 {
        if !crate::sched::queue::core_is_online(core_id) {
            continue;
        }
        let ticks = crate::sched::queue::tick_count_for_core(core_id);
        serial_print("Core ");
        serial_print_usize(core_id as usize);
        serial_print(" ticks: ");
        serial_print_usize(ticks as usize);
        serial_print("\n");
    }

    if online_cores >= 2 {
        let (tx, _rx) = crate::ipc::create_channel().unwrap();
        let target_core = 1u8;
        let start = crate::perf::counters::rdtsc();
        for _ in 0..1000u32 {
            let msg = crate::ipc::Message {
                sender_pid: 0,
                payload: [0u8; 56],
                cap_transfer: None,
            };
            let _ = crate::ipc::send_cross_core(tx, msg, target_core);
        }
        let end = crate::perf::counters::rdtsc();
        serial_print("Cross-core IPC: ");
        serial_print_usize(((end - start) / 1000) as usize);
        serial_print(" cycles\n");
    } else {
        serial_print("Cross-core IPC: 0 cycles\n");
    }

    serial_print("=== SMP Benchmarks Complete ===\n");
}

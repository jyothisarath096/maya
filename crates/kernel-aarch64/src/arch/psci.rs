const PSCI_CPU_ON_64: u64 = 0xC400_0003;
const PSCI_CPU_OFF: u64 = 0x8400_0002;
const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;
const PSCI_SYSTEM_RESET: u64 = 0x8400_0009;
const PSCI_SUCCESS: i64 = 0;
const PHYS_TO_VIRT_OFFSET: u64 = 0xFFFF_0000_0000_0000;

pub fn cpu_on(target_cpu: u64, entry_point: u64, context_id: u64) -> i64 {
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "hvc #0",
            inout("x0") PSCI_CPU_ON_64 => ret,
            in("x1") target_cpu,
            in("x2") entry_point,
            in("x3") context_id,
            options(nomem, nostack)
        );
    }
    ret
}

pub fn system_off() -> ! {
    let _ = PSCI_CPU_OFF;
    unsafe {
        core::arch::asm!(
            "hvc #0",
            in("x0") PSCI_SYSTEM_OFF,
            options(nomem, nostack, noreturn)
        );
    }
}

pub fn system_reset() -> ! {
    unsafe {
        core::arch::asm!(
            "hvc #0",
            in("x0") PSCI_SYSTEM_RESET,
            options(nomem, nostack, noreturn)
        );
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ap_entry_rust(cpu: u64) -> ! {
    let cpu = cpu as u8;
    crate::arch::cpu::enable_fp_simd();
    crate::arch::gic::init_cpu_interface();
    crate::sched::queue::init_core(cpu);

    crate::uart_print!("AP online: core ");
    crate::uart_print_usize!(cpu as usize);
    crate::uart_print!("\n");

    crate::arch::timer::init_ap();
    crate::arch::timer::enable_ap_timer();
    unsafe {
        core::arch::asm!(
            "msr daifclr, #2",
            "isb",
            options(nomem, nostack, preserves_flags)
        );
    }
    loop {
        unsafe {
            core::arch::asm!(
                "wfe",
                options(nomem, nostack, preserves_flags)
            );
        }
    }
}

pub fn start_all_aps() {
    unsafe extern "C" {
        fn ap_trampoline();
    }
    let ap_entry_virt = ap_trampoline as *const () as u64;
    let ap_entry_phys = ap_entry_virt.wrapping_sub(PHYS_TO_VIRT_OFFSET);
    for cpu in 1u64..8 {
        let result = cpu_on(cpu, ap_entry_phys, cpu);
        if result == PSCI_SUCCESS {
            match cpu {
                1 => crate::uart_print!("PSCI: core 1 started\n"),
                2 => crate::uart_print!("PSCI: core 2 started\n"),
                3 => crate::uart_print!("PSCI: core 3 started\n"),
                4 => crate::uart_print!("PSCI: core 4 started\n"),
                5 => crate::uart_print!("PSCI: core 5 started\n"),
                6 => crate::uart_print!("PSCI: core 6 started\n"),
                7 => crate::uart_print!("PSCI: core 7 started\n"),
                _ => crate::uart_print!("PSCI: core ? started\n"),
            }
        } else {
            match cpu {
                1 => crate::uart_print!("PSCI: core 1 failed\n"),
                2 => crate::uart_print!("PSCI: core 2 failed\n"),
                3 => crate::uart_print!("PSCI: core 3 failed\n"),
                4 => crate::uart_print!("PSCI: core 4 failed\n"),
                5 => crate::uart_print!("PSCI: core 5 failed\n"),
                6 => crate::uart_print!("PSCI: core 6 failed\n"),
                7 => crate::uart_print!("PSCI: core 7 failed\n"),
                _ => crate::uart_print!("PSCI: core ? failed\n"),
            }
        }
    }
}

#![allow(dead_code)]

use bootloader_api::BootInfo;
use x86_64::VirtAddr;

use crate::{KernelError, serial_print, serial_print_usize};

pub mod heap;
pub mod pmm;
pub mod vmm;

pub fn init(boot_info: &'static mut BootInfo) -> Result<(), KernelError> {
    pmm::init(&boot_info.memory_regions);
    serial_print_from_memory("PMM initialised\n");

    let s = pmm::stats();
    serial_print("PMM frames total=");
    serial_print_usize(s.total_frames);
    serial_print(" free=");
    serial_print_usize(s.free_frames);
    serial_print("\n");

    let offset_opt = boot_info.physical_memory_offset.into_option();
    match offset_opt {
        None => {
            return Err(KernelError::VmmNotInitialized);
        }
        Some(offset) => {
            let phys_mem_offset = VirtAddr::new(offset);
            if let Err(_) = vmm::init(phys_mem_offset) {
                return Err(KernelError::VmmNotInitialized);
            }
            serial_print("VMM initialised\n");
            if let Some(rsdp) = boot_info.rsdp_addr.into_option() {
                let cpu_count = crate::arch::smp::detect_cpus(rsdp);
                serial_print("CPUs detected: ");
                serial_print_usize(cpu_count);
                serial_print("\n");
            }
            heap::init(phys_mem_offset)?;
            serial_print("Heap initialised\n");
        }
    }

    Ok(())
}

fn serial_print_from_memory(s: &str) {
    crate::serial_print(s);
}

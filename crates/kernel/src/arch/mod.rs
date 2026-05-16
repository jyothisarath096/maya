pub mod gdt;
pub mod idt;
pub mod interrupts;
pub mod secure;
pub mod smp;

use crate::KernelError;

pub fn init() -> Result<(), KernelError> {
    gdt::init();
    idt::init();
    x86_64::instructions::interrupts::enable();
    Ok(())
}

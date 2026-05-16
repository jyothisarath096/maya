pub fn enable_fp_simd() {
    unsafe {
        core::arch::asm!(
            "mrs x0, cpacr_el1",
            "orr x0, x0, #(0x3 << 20)",
            "msr cpacr_el1, x0",
            "isb",
            out("x0") _,
            options(nomem, nostack)
        );
    }
}

pub fn current_core_id() -> u8 {
    let mpidr: u64;
    unsafe {
        core::arch::asm!(
            "mrs {mpidr}, mpidr_el1",
            mpidr = out(reg) mpidr,
            options(nomem, nostack, preserves_flags)
        );
    }
    (mpidr & 0xFF) as u8
}

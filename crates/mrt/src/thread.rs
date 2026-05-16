pub fn yield_now() {
    unsafe {
        core::arch::asm!(
            "mov x8, #0x01",
            "svc #0",
            out("x8") _,
            options(nostack)
        );
    }
}

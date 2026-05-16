#[inline(always)]
pub unsafe fn syscall0(nr: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "svc #0",
        in("x8") nr,
        lateout("x0") ret,
        lateout("x1") _,
        lateout("x2") _,
        lateout("x3") _,
        lateout("x4") _,
        lateout("x5") _,
        lateout("x6") _,
        lateout("x7") _,
        lateout("x8") _,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall3(nr: u64, a0: u64, a1: u64, a2: u64) -> (i64, i64) {
    let ret0: i64;
    let ret1: i64;
    core::arch::asm!(
        "svc #0",
        in("x8") nr,
        in("x0") a0,
        in("x1") a1,
        in("x2") a2,
        lateout("x0") ret0,
        lateout("x1") ret1,
        lateout("x2") _,
        lateout("x3") _,
        lateout("x4") _,
        lateout("x5") _,
        lateout("x6") _,
        lateout("x7") _,
        lateout("x8") _,
        options(nostack)
    );
    (ret0, ret1)
}

#[inline(always)]
pub unsafe fn syscall4(nr: u64, a0: u64, a1: u64, a2: u64, a3: u64) -> (i64, i64) {
    let ret0: i64;
    let ret1: i64;
    core::arch::asm!(
        "svc #0",
        in("x8") nr,
        in("x0") a0,
        in("x1") a1,
        in("x2") a2,
        in("x3") a3,
        lateout("x0") ret0,
        lateout("x1") ret1,
        lateout("x2") _,
        lateout("x3") _,
        lateout("x4") _,
        lateout("x5") _,
        lateout("x6") _,
        lateout("x7") _,
        lateout("x8") _,
        options(nostack)
    );
    (ret0, ret1)
}

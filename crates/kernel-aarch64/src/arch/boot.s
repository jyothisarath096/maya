.section .text.boot
.global _start

.extern __bss_start
.extern __bss_end
.extern exception_vectors

_start:
    mrs x0, mpidr_el1
    and x0, x0, #0xFF
    cbnz x0, cpu_idle

    mrs x0, CurrentEL
    lsr x0, x0, #2
    and x0, x0, #3
    cmp x0, #2
    beq el2_to_el1
    b boot_el1

el2_to_el1:
    mov x0, #(1 << 31)
    msr hcr_el2, x0
    isb

    mov x0, #0x0800
    msr sctlr_el1, x0
    isb

    mov x0, #0x3C5
    msr spsr_el2, x0

    adr x0, boot_el1
    msr elr_el2, x0
    isb
    eret

boot_el1:
    ldr x0, =0x40100000
    mov sp, x0

    ldr x0, =__bss_start
    ldr x1, =__bss_end
    ldr x2, =0xFFFF000000000000
    sub x0, x0, x2
    sub x1, x1, x2
    cmp x0, x1
    beq bss_done
bss_loop:
    str xzr, [x0], #8
    cmp x0, x1
    blt bss_loop
bss_done:
    ldr x0, =0x40180000
    mov x1, #4096
    mov x2, xzr
zero_l0:
    str x2, [x0], #8
    subs x1, x1, #8
    bne zero_l0

    ldr x0, =0x40181000
    mov x1, #4096
    mov x2, xzr
zero_l1:
    str x2, [x0], #8
    subs x1, x1, #8
    bne zero_l1

    ldr x0, =0x40182000
    mov x1, #4096
    mov x2, xzr
zero_l2:
    str x2, [x0], #8
    subs x1, x1, #8
    bne zero_l2

    ldr x0, =0x40183000
    mov x1, #4096
    mov x2, xzr
zero_ttbr0_l0:
    str x2, [x0], #8
    subs x1, x1, #8
    bne zero_ttbr0_l0

    ldr x0, =0x40184000
    mov x1, #4096
    mov x2, xzr
zero_ttbr0_l1:
    str x2, [x0], #8
    subs x1, x1, #8
    bne zero_ttbr0_l1

    ldr x0, =0x40180000
    ldr x1, =0x40181000
    orr x1, x1, #3
    str x1, [x0]

    ldr x0, =0x40181000
    ldr x1, =0x40000000
    ldr x2, =0x0000000000000401
    orr x2, x2, x1
    str x2, [x0, #8]

    ldr x0, =0x40181000
    ldr x1, =0x40182000
    orr x1, x1, #3
    str x1, [x0]

    ldr x0, =0x40182000
    ldr x1, =0x09000000
    ldr x2, =0x0060000000000401
    orr x2, x2, x1
    str x2, [x0, #(0x48 * 8)]

    ldr x0, =0x40182000
    ldr x1, =0x0A000000
    ldr x2, =0x0060000000000401
    orr x2, x2, x1
    str x2, [x0, #(0x50 * 8)]

    ldr x0, =0x40182000
    ldr x1, =0x08000000
    ldr x2, =0x0060000000000401
    orr x2, x2, x1
    str x2, [x0, #(0x40 * 8)]

    ldr x0, =0x40183000
    ldr x1, =0x40184000
    orr x1, x1, #3
    str x1, [x0]

    ldr x0, =0x40184000
    ldr x1, =0x40000000
    ldr x2, =0x0000000000000401
    orr x2, x2, x1
    str x2, [x0, #8]

    dsb sy
    isb

    ldr x0, =0x00000000000044FF
    msr mair_el1, x0

    ldr x0, =0x00000000B5103510
    msr tcr_el1, x0
    isb

    ldr x0, =0x40180000
    msr ttbr1_el1, x0

    ldr x0, =0x40183000
    msr ttbr0_el1, x0

    tlbi vmalle1
    dsb ish
    isb

    ldr x1, =0x09000000
    mov w0, #0x50
    str w0, [x1]
    mov w0, #0x52
    str w0, [x1]
    mov w0, #0x45
    str w0, [x1]
    mov w0, #0x0D
    str w0, [x1]
    mov w0, #0x0A
    str w0, [x1]

    mrs x0, sctlr_el1
    ldr x1, =0x00000001
    orr x0, x0, x1
    msr sctlr_el1, x0
    isb

    ldr x0, =virt_entry
    br x0

virt_entry:
    ldr x0, =0xFFFF000040100000
    mov sp, x0

    ldr x0, =exception_vectors
    msr vbar_el1, x0
    isb

    ldr x1, =0xFFFF000009000000
    mov w0, #0x56
    str w0, [x1]
    mov w0, #0x49
    str w0, [x1]
    mov w0, #0x52
    str w0, [x1]
    mov w0, #0x54
    str w0, [x1]
    mov w0, #0x0D
    str w0, [x1]
    mov w0, #0x0A
    str w0, [x1]

    bl kernel_main
    b cpu_idle

cpu_idle:
    wfe
    b cpu_idle

.global ap_trampoline
ap_trampoline:
    mov x19, x0

    mov x1, #0x10000
    mul x1, x19, x1
    ldr x2, =0x40100000
    sub x2, x2, x1
    mov sp, x2

    mov x0, #0x44FF
    msr mair_el1, x0

    mov x0, #0x3510
    movk x0, #0xB510, lsl #16
    msr tcr_el1, x0
    isb

    mov x0, #0x0000
    movk x0, #0x4018, lsl #16
    msr ttbr1_el1, x0

    mov x0, #0x3000
    movk x0, #0x4018, lsl #16
    msr ttbr0_el1, x0

    tlbi vmalle1
    dsb ish
    isb

    mrs x0, sctlr_el1
    orr x0, x0, #1
    msr sctlr_el1, x0
    isb

    ldr x0, =1f
    br x0
1:
    mov x1, #0x10000
    mul x1, x19, x1
    ldr x2, =0xFFFF000040100000
    sub x2, x2, x1
    mov sp, x2

    ldr x0, =exception_vectors
    msr vbar_el1, x0
    isb

    mov x0, x19
    b ap_entry_rust

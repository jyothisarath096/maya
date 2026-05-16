.section .text
.global irq_entry_el0

irq_entry_el0:
    sub sp, sp, #16
    stp x18, x19, [sp]

    mrs x18, tpidr_el1

    stp x0,  x1,  [x18, #0]
    stp x2,  x3,  [x18, #16]
    stp x4,  x5,  [x18, #32]
    stp x6,  x7,  [x18, #48]
    stp x8,  x9,  [x18, #64]
    stp x10, x11, [x18, #80]
    stp x12, x13, [x18, #96]
    stp x14, x15, [x18, #112]
    stp x16, x17, [x18, #128]

    ldr x19, [sp, #0]
    str x19, [x18, #144]
    ldr x19, [sp, #8]
    str x19, [x18, #152]
    stp x20, x21, [x18, #160]
    stp x22, x23, [x18, #176]
    stp x24, x25, [x18, #192]
    stp x26, x27, [x18, #208]
    str x28,      [x18, #224]
    stp x29, x30, [x18, #232]

    mrs x0, elr_el1
    mrs x1, spsr_el1
    mrs x2, sp_el0
    stp x0, x1, [x18, #248]
    str x2,     [x18, #264]

    stp q0,  q1,  [x18, #288]
    stp q2,  q3,  [x18, #320]
    stp q4,  q5,  [x18, #352]
    stp q6,  q7,  [x18, #384]
    stp q8,  q9,  [x18, #416]
    stp q10, q11, [x18, #448]
    stp q12, q13, [x18, #480]
    stp q14, q15, [x18, #512]
    stp q16, q17, [x18, #544]
    stp q18, q19, [x18, #576]
    stp q20, q21, [x18, #608]
    stp q22, q23, [x18, #640]
    stp q24, q25, [x18, #672]
    stp q26, q27, [x18, #704]
    stp q28, q29, [x18, #736]
    stp q30, q31, [x18, #768]

    mrs x0, fpcr
    mrs x1, fpsr
    str x0, [x18, #800]
    str x1, [x18, #808]

    mov x0, x18
    .inst 0xd500419f
    isb
    bl irq_context_switch_handler
    mov x18, x0

    ldp q0,  q1,  [x18, #288]
    ldp q2,  q3,  [x18, #320]
    ldp q4,  q5,  [x18, #352]
    ldp q6,  q7,  [x18, #384]
    ldp q8,  q9,  [x18, #416]
    ldp q10, q11, [x18, #448]
    ldp q12, q13, [x18, #480]
    ldp q14, q15, [x18, #512]
    ldp q16, q17, [x18, #544]
    ldp q18, q19, [x18, #576]
    ldp q20, q21, [x18, #608]
    ldp q22, q23, [x18, #640]
    ldp q24, q25, [x18, #672]
    ldp q26, q27, [x18, #704]
    ldp q28, q29, [x18, #736]
    ldp q30, q31, [x18, #768]

    ldr x0, [x18, #800]
    ldr x1, [x18, #808]
    msr fpcr, x0
    msr fpsr, x1

    ldp x0, x1, [x18, #248]
    ldr x2,     [x18, #264]
    msr elr_el1, x0
    msr spsr_el1, x1
    msr sp_el0, x2

    ldp x0,  x1,  [x18, #0]
    ldp x2,  x3,  [x18, #16]
    ldp x4,  x5,  [x18, #32]
    ldp x6,  x7,  [x18, #48]
    ldp x8,  x9,  [x18, #64]
    ldp x10, x11, [x18, #80]
    ldp x12, x13, [x18, #96]
    ldp x14, x15, [x18, #112]
    ldp x16, x17, [x18, #128]
    ldr x19,      [x18, #152]
    ldp x20, x21, [x18, #160]
    ldp x22, x23, [x18, #176]
    ldp x24, x25, [x18, #192]
    ldp x26, x27, [x18, #208]
    ldr x28,      [x18, #224]
    ldp x29, x30, [x18, #232]
    ldr x18,      [x18, #144]

    add sp, sp, #16
    eret

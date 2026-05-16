.section .text
.global exception_vectors
.global el0_sync_handler
.global el0_fault_handler
.balign 2048

exception_vectors:

// Group 1: Current EL, SP_EL0
.balign 128
b sync_handler_el1
.balign 128
b irq_handler_el1
.balign 128
b fiq_handler_el1
.balign 128
b serror_handler_el1

// Group 2: Current EL, SP_ELx
.balign 128
b sync_handler_el1
.balign 128
b irq_handler_el1
.balign 128
b fiq_handler_el1
.balign 128
b serror_handler_el1

// Group 3: Lower EL, AArch64
.balign 128
b el0_sync_handler
.balign 128
b irq_entry_el0
.balign 128
b fiq_handler_el1
.balign 128
b serror_handler_el1

// Group 4: Lower EL, AArch32
.balign 128
b sync_handler_el1
.balign 128
b irq_handler_el1
.balign 128
b fiq_handler_el1
.balign 128
b serror_handler_el1

el0_sync_handler:
sub sp, sp, #(35 * 8)
stp x0,  x1,  [sp, #0]
stp x2,  x3,  [sp, #16]
stp x4,  x5,  [sp, #32]
stp x6,  x7,  [sp, #48]
stp x8,  x9,  [sp, #64]
stp x10, x11, [sp, #80]
stp x12, x13, [sp, #96]
stp x14, x15, [sp, #112]
stp x16, x17, [sp, #128]
stp x18, x19, [sp, #144]
stp x20, x21, [sp, #160]
stp x22, x23, [sp, #176]
stp x24, x25, [sp, #192]
stp x26, x27, [sp, #208]
stp x28, x29, [sp, #224]
str x30,      [sp, #240]
mrs x9, esr_el1
lsr x10, x9, #26
cmp x10, #0x15
bne el0_restore_and_fault
mrs x9, elr_el1
mrs x10, spsr_el1
mrs x11, sp_el0
stp x9, x10, [sp, #248]
str x11,     [sp, #264]
mov x0, sp
msr daifset, #2
isb
dsb sy
isb
.inst 0xd500419f
isb
bl svc_handler_el0
ldp x9, x10, [sp, #248]
ldr x11,     [sp, #264]
msr elr_el1, x9
msr spsr_el1, x10
msr sp_el0,  x11
ldp x0,  x1,  [sp, #0]
ldp x2,  x3,  [sp, #16]
ldp x4,  x5,  [sp, #32]
ldp x6,  x7,  [sp, #48]
ldp x8,  x9,  [sp, #64]
ldp x10, x11, [sp, #80]
ldp x12, x13, [sp, #96]
ldp x14, x15, [sp, #112]
ldp x16, x17, [sp, #128]
ldp x18, x19, [sp, #144]
ldp x20, x21, [sp, #160]
ldp x22, x23, [sp, #176]
ldp x24, x25, [sp, #192]
ldp x26, x27, [sp, #208]
ldp x28, x29, [sp, #224]
ldr x30,      [sp, #240]
add sp, sp, #(35 * 8)
eret

el0_restore_and_fault:
ldp x8,  x9,  [sp, #64]
ldp x10, x11, [sp, #80]
add sp, sp, #(35 * 8)
b el0_fault_handler

el0_fault_handler:
bl fault_handler_el0
1:
b 1b

.section .text
.global _start
.global hello_fn
.global write_msg

_start:
    bl hello_fn
    mov x8, #0x00
    mov x0, #0
    svc #0

hello_fn:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    bl write_msg
    ldp x29, x30, [sp], #16
    ret

write_msg:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    mov x8, #0x101
    mov x0, #1
    adr x1, msg
    mov x2, #14
    svc #0
    ldp x29, x30, [sp], #16
    ret

.section .data
msg:
    .ascii "Hello AArch64\n"

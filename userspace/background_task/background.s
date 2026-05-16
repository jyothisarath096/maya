.section .text
.global _start
.global background_fn
.global background_work

_start:
loop:
    bl background_fn
    mov x8, #0x01
    svc #0
    b loop

background_fn:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    bl background_work
    ldp x29, x30, [sp], #16
    ret

background_work:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    mov x0, #10000
1:
    subs x0, x0, #1
    bne 1b
    ldp x29, x30, [sp], #16
    ret

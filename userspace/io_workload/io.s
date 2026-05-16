.section .text
.global _start
.global io_fn
.global io_wait

_start:
loop:
    bl io_fn
    mov x8, #0x01
    svc #0
    b loop

io_fn:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    bl io_wait
    ldp x29, x30, [sp], #16
    ret

io_wait:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    mov x0, #5000
1:
    subs x0, x0, #1
    bne 1b
    ldp x29, x30, [sp], #16
    ret

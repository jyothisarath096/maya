.section .text
.global _start
.global compute_fn
.global compute_heavy

_start:
loop:
    bl compute_fn
    mov x8, #0x01
    svc #0
    b loop

compute_fn:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    bl compute_heavy
    ldp x29, x30, [sp], #16
    ret

compute_heavy:
    stp x29, x30, [sp, #-16]!
    mov x29, sp
    mov x0, #1000
1:
    subs x0, x0, #1
    bne 1b
    ldp x29, x30, [sp], #16
    ret

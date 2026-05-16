section .text
global _start

_start:
    ; Call SYS_YIELD (syscall 4)
    mov rax, 4
    syscall

    ; Get stdout cap
    mov rax, 5
    mov rdi, 0
    syscall
    mov r15, rax

    ; Write "Y\n" via SYS_WRITE
    mov rax, 1
    mov rdi, r15
    lea rsi, [rel msg]
    mov rdx, msg_len
    syscall

    ; Exit
    mov rax, 0
    xor rdi, rdi
    syscall

section .data
msg:     db "Yield test OK!", 10
msg_len: equ $ - msg

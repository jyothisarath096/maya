section .text
global _start
extern main

_start:
    xor rbp, rbp
    call main
    mov rdi, rax
    mov rax, 0      ; SYS_EXIT
    syscall

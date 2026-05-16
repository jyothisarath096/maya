; Maya userspace hello world (assembly)
; Build: nasm -f elf64 hello.asm -o hello.o
;        x86_64-elf-ld hello.o -o hello_asm

section .text
global _start

_start:
    ; Get stdout capability (syscall 5, arg=0)
    mov rax, 5
    mov rdi, 0
    syscall
    mov r15, rax        ; save stdout cap

    ; Write message (syscall 1)
    mov rax, 1
    mov rdi, r15        ; stdout cap
    lea rsi, [rel msg]
    mov rdx, msg_len
    syscall

    ; Exit (syscall 0)
    mov rax, 0
    xor rdi, rdi
    syscall

section .data
msg:     db "Hello from Maya userspace!", 10
msg_len: equ $ - msg

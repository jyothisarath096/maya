# Phase 10: AArch64 ARM Port
## Status: TASK 1 COMPLETE — Boot + Full SMP
## Date: 2026-03-26

## Verified Boot Output
Maya AArch64 booting...
PMM initialised
Exception vectors installed
MMU configured
GIC-v2 initialised
Timer initialised at 100Hz (BSP core 0)
PSCI: core 1 started
AP online: core 1
... (cores 2-7 all online)
Maya AArch64 ready
maya-arm>

## What Works
- PL011 UART at 0x09000000 with spinlock
- Bitmap PMM (fixed QEMU virt RAM map)
- AArch64 exception vectors (VBAR_EL1)
- MMU (TCR_EL1, MAIR_EL1 configured)
- GIC-v2 (GICD + GICC memory mapped)
- Virtual Timer at 100Hz (CNTV_*_EL0)
- PSCI CPU_ON — all 8 cores online
- Per-core timer init on each AP
- UART spinlock for multi-core output

## SMP Result
8 cores online via PSCI on first attempt.
This resolves the Phase 8 SMP blocker
that was never solved on x86-64.

## Build Command
  ./scripts/run-aarch64.sh

## QEMU Command
  qemu-system-aarch64 \
    -machine virt,gic-version=2 \
    -cpu cortex-a72 \
    -smp 8 \
    -m 256M \
    -device loader,file=kernel.bin,\
      addr=0x40200000,cpu-num=0 \
    -serial stdio -display none -no-reboot

## ARM vs x86-64 Comparison
| Feature          | x86-64 result | AArch64 result |
|------------------|---------------|----------------|
| AP startup       | 0/4 cores     | 7/7 APs online |
| Timer per core   | 1 core only   | 8 cores        |
| SMP mechanism    | SIPI (blocked)| PSCI (works)   |
| Boot time        | ~2 seconds    | <1 second      |

## Next Tasks
- Port capability system
- Port PPO scheduler
- Port I/O mediator
- Port IPC
- Port natural language shell
- Port userspace (AArch64 syscall ABI)
- Port MAR (AArch64 prologue patterns)

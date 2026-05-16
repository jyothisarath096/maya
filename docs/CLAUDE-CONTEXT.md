# Maya OS — Complete Context for Claude

## Project
Building Maya OS from scratch in Rust — an AI-native operating system.
End goal: Apple-like ecosystem with tight hardware-software integration.
Author: Kolaparthi Jyothi Sarath, Independent Researcher
Published paper: https://doi.org/10.5281/zenodo.19218503

## Repository
/Users/buddhi/Desktop/maya/
Archive: /Users/buddhi/Desktop/maya-x86-phase9-archive/

## Completed Phases (x86-64)
Phase 0-9 complete. See docs/phase8-smp-status.md for details.

Key: all x86-64 kernel code in crates/kernel/
MAR tool: tools/mar/mar.py
AArch64 kernel: crates/kernel-aarch64/

## Current Status: Phase 10 AArch64
- Boot working on QEMU virt machine
- 8-core SMP via PSCI working
- GIC-v2, Generic Timer, UART all working
- Run with: ./scripts/run-aarch64.sh

## Next Task
Port Phase 1-9 logic to AArch64:
1. Capability system
2. PPO scheduler
3. I/O mediator
4. IPC
5. Userspace ABI (AArch64 syscalls)
6. MAR AArch64 shim (different prologue patterns)

## Key People
- Codex: writes kernel Rust code
- Gemini: architecture/design questions only
- Claude: decisions, review, verification
- Buddhi: builds, boots, verifies on hardware

## Build Commands
x86-64: cargo +nightly rustc -Zjson-target-spec
  -Zbuild-std=core,alloc,compiler_builtins
  -Zbuild-std-features=compiler-builtins-mem
  --target targets/x86_64-aios.json
  -p kernel --bin kernel --offline

AArch64: ./scripts/run-aarch64.sh

## Hardware
- Dev machine: Apple Silicon M4 MacBook Air
- Test machine: Dell Inspiron (Intel Core, 8 cores, 16GB)
- USB boot: SanDisk at /dev/disk6s2 MAYA_EFI

## Codex Protocol
- Never run cargo add/fetch
- Never request network access
- Report: TASK X.Y COMPLETE, BLOCKED, or SPEC GAP
- Canonical repo: /Users/buddhi/Desktop/maya

## Phase 10 AArch64 — COMPLETE
All tasks 10.1-10.7 done. Archive: maya-phase10-aarch64-archive

## Phase 11 Next Tasks
1. EL0/EL1 context switch (save x0-x30, NEON, ELR, SPSR, SP_EL0)
2. Telemetry harvest via: ./scripts/run-aarch64.sh 2>&1 | grep "^T," > telemetry.csv
3. Python gym environment (OpenAI Gym, 100Hz Maya simulator)
4. PPO training via MLX on M4 Neural Engine
5. Embed trained weights in kernel-aarch64

## AArch64 Syscall ABI (final)
x8=nr, x0-x7=args, x0=ret_lo, x1=ret_hi
0x00=EXIT, 0x01=YIELD, 0x80=INTENT_REG, 0x88=TELEMETRY
0x100+=mediated I/O (auto-mediator invoked)

## MAR AArch64
tools/mar/mar_aarch64.py — run in tools/mar/venv
SHIM_LOAD_ADDR=0x04000000
Prologue mask: (word & 0xFFC07FFF) == 0xA9807BFD
Test binary: userspace/hello_aarch64/hello_aarch64

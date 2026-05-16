# Maya OS Architecture

## Overview
Maya OS is an AI-native AArch64 bare-metal operating system written in Rust with `no_std`.
It targets the QEMU `virt` machine and currently runs on 8 cores with 256 MiB of RAM.

The system is built around three ideas:

1. Use the hardware directly rather than abstracting it away.
2. Make process behavior observable through telemetry, intents, and capabilities.
3. Let scheduling policy evolve online via a lightweight PPO-style update path.

Maya combines a small SMP kernel, a capability system, an intent-aware scheduler, an in-kernel IO mediator, a custom MAR instrumentation pipeline, and a minimal MRT userspace runtime.

## Cardinal Principles

### P1. Full AArch64 Hardware Exploitation
Maya is designed to use the platform directly:

- EL1 kernel / EL0 userspace split
- TTBR0/TTBR1 virtual memory separation
- PSCI for secondary-core bring-up
- GIC-v2 for interrupts and SGI inter-core wakeups
- Architectural timers for preemptive scheduling
- PAN enforced on EL0 exception entry
- `ldtrb` / `sttrb` for controlled EL1 access to EL0 memory

### P2. AI-Native Operation
Maya treats policy as a first-class part of the OS:

- processes declare intent classes (`Compute`, `IO`, `RealTime`, `Background`, `System`)
- syscalls emit telemetry
- IO is capability-gated and mediated
- the scheduler uses a PPO-style model
- the PPO output layer is updated online from runtime reward signals

## Platform Model

- Architecture: AArch64
- Privilege levels used: EL1 kernel, EL0 userspace
- Machine: QEMU `virt`
- Cores: 8
- Interrupt controller: GIC-v2
- Core bring-up: PSCI `CPU_ON`
- Timer model:
  - core 0 uses the virtual timer path
  - AP cores use the physical timer path after SGI wakeup

## Memory Map

### Kernel
- Kernel virtual base: `0xFFFF000040200000`
- Kernel physical base: `0x40200000`

### Userspace ELF Slots
- ELF slot base: `0x41800000`
- ELF slot stride: `0x00100000` (1 MiB per slot)
- Formula: `0x41800000 + slot * 0x00100000`

Current notable placements:

- slot 0: `0x41800000` - `compute_workload`
- slot 1: `0x41900000` - `io_workload`
- slot 2: `0x41A00000` - `background_task`
- slot 3: `0x41B00000` - `matrix_multiply`
- slot 4+: reserved / legacy lower region
- high region:
  - `0x42000000` - `mrt_producer`
  - `0x42100000` - `mrt_consumer`
  - `0x43000000` - `net_parser`
  - `0x43100000` - `sort_suite`

### Userspace Stacks
- Stack base: `0x50000000`
- Stack stride: `0x00010000` (64 KiB per slot)
- Formula: `0x50000000 + slot * 0x00010000`

### MAR Shim Region
- Shim base: `0x47F00000`
- Shim stride: `0x00010000`
- Formula: `0x47F00000 + slot * 0x00010000`

### Userspace Dynamic Allocation
- Per-process allocation base: `0x43000000`
- Per-process allocation stride: `0x01000000`
- Allocations are capability-backed and issued by `SYS_ALLOC` / `SYS_FREE`

### MRT Heap Notes
- MRT programs allocate through kernel-backed `MrtAlloc`
- The current task convention treats MRT heap activity as starting from the `0x50000000` userspace region family
- In practice, heap objects are obtained through the kernel allocator rather than a fixed bump heap in userspace

## Process Model

Each process has:

- a PID
- an ELF entry point
- an EL0 stack top
- TTBR0 + ASID
- a kernel-owned saved context frame
- capability slots
- registered intents
- per-process allocations
- runtime statistics used by scheduling and reward computation

Important implementation facts:

- PID allocation currently starts at `4`
- ASIDs are assigned incrementally
- each process has an embedded 8 KiB kernel stack
- process classes are:
  - `Idle`
  - `Batch`
  - `Interactive`
  - `Realtime`

## Process Table (Current 9-Process Deployment)

| PID | Name | Core | Class | Notes |
| --- | --- | --- | --- | --- |
| 4 | `compute` | 2 | `Interactive` | legacy C workload |
| 5 | `io` | 3 | `Interactive` | legacy C workload |
| 6 | `bg` | 4 | `Batch` | background workload |
| 7 | `matrix` | 5 | `Interactive` | matrix multiply workload |
| 8 | `net_parser` | 6 | `Interactive` | relocated to high ELF range |
| 9 | `sort_suite` | 7 | `Batch` | relocated to high ELF range |
| 10 | `mrt_hello` | 1 | `Realtime` | MRT runtime validation workload |
| 11 | `mrt_producer` | 0 | `Realtime` | sends structured sensor messages |
| 12 | `mrt_consumer` | 0 | `Realtime` | receives sensor data and returns alarm ACKs |

## Syscall Architecture

Maya groups syscalls into five families:

- core control
- capabilities
- IPC
- intents / telemetry / injection
- IO and allocation

The main dispatcher lives in `crates/kernel-aarch64/src/proc/syscall.rs`.

## Syscall Table

### Core Syscalls

| Number | Name | Purpose |
| --- | --- | --- |
| `0x00` | `SYS_NOP` | reserved no-op |
| `0x01` | `SYS_YIELD` | voluntary yield and scheduler handoff |
| `0x02` | `SYS_EXIT` | currently stubbed / not implemented |
| `0x03` | `SYS_GETPID` | returns caller-supplied pid-style value in current stub |
| `0x04` | reserved | currently stubbed |

### Capability Syscalls

| Number | Name | Purpose |
| --- | --- | --- |
| `0x10` | `SYS_CAP_CREATE` | create a capability |
| `0x11` | `SYS_CAP_REVOKE` | revoke a capability |
| `0x12` | `SYS_CAP_DELEGATE` | delegate a capability to another PID |
| `0x13` | `SYS_CAP_CHECK` | verify rights on a capability |

### IPC Syscalls

| Number | Name | Purpose |
| --- | --- | --- |
| `0x20` | `SYS_CHAN_CREATE` | create a channel pair |
| `0x21` | `SYS_CHAN_SEND` | send payload bytes to a channel |
| `0x22` | `SYS_CHAN_RECV` | receive payload bytes directly into user memory |
| `0x23` | `SYS_CHAN_RECV_NB` | nonblocking recv returning packed immediate data |
| `0x123` | `SYS_CHAN_LOOKUP_RECV` | lookup recv capability for caller |
| `0x124` | `SYS_CHAN_LOOKUP_SEND` | lookup send capability for caller |

### Intent / Telemetry / Injection Syscalls

| Number | Name | Purpose |
| --- | --- | --- |
| `0x80` | `SYS_INTENT_REG` | register process intent and obtain intent capability |
| `0x88` | `SYS_TELEMETRY` | emit intent telemetry and refresh process intent state |
| `0x89` | `SYS_INJECT_RETURN` | complete an injected call and yield |

### IO / Allocation Syscalls

| Number | Name | Purpose |
| --- | --- | --- |
| `0x100` | `SYS_READ` | mediated file read event |
| `0x101` | `SYS_WRITE` | mediated console/file write; current kernel console path writes UART directly |
| `0x102` | `SYS_OPEN` | mediated file open event |
| `0x103` | `SYS_NET_SEND` | mediated network send event |
| `0x104` | `SYS_NET_RECV` | mediated network recv event |
| `0x105` | `SYS_MMAP` | mediated memory map event |
| `0x110` | `SYS_ALLOC` | capability-backed userspace allocation |
| `0x111` | `SYS_FREE` | free allocation by capability |
| `0x114` | `SYS_CHAN_PIPE` | create self-owned channel pair |
| `0x115` | `SYS_GET_CONSOLE_CAP` | return process console capability |
| `0x116` | `SYS_CHAN_GRANT` | delegate channel rights to another PID |

## Components

### SMP Bring-Up
- secondary cores are started through PSCI `CPU_ON`
- APs initialize FP/SIMD, the per-core GIC CPU interface, a scheduler queue, and the AP timer path
- APs idle in `wfe` until work arrives

### GIC-v2 and Inter-Core Wakeup
- SGI ID `1` is used for inter-core process wakeup
- `send_ipi()` targets a specific CPU through `GICD_SGIR`
- APs receive SGIs, run `handle_ipc_sgi()`, and launch or resume queued work
- timer interrupt ID `27` drives preemption

### PPO Scheduler
- scheduling operates per core with process classes and dynamic statistics
- features include CPU use, wait time, IO wait, IPC rates, memory pressure, page faults, burst behavior, deadline urgency, intent weight, starvation risk, and capability pressure
- the inference path uses a frozen lower network and a mutable output layer
- online learning updates the output layer periodically from reward snapshots
- `PPO,<core>,<reward>` telemetry exposes live learning updates

### MAR (Maya Agentic Recompiler)
- userspace binaries are post-processed into:
  - `.mexe` executable image
  - `.mshm` shim image
  - `.mlmb` metadata blob
- `_start` is now intentionally skipped during hook generation
- MAR shims are used for intent telemetry and function-level observability

### Capability System
- capabilities carry ownership, resource type, rights, and generation metadata
- resource types include memory, channels, process, interrupt, intent, telemetry, network, and crypto-class resources
- rights are checked in kernel code before privileged operations
- channel lookup helpers resolve channel caps by owner PID and channel resource ID

### IPC Channels
- channels are fixed-size kernel objects with capability-gated send and receive ends
- the current message payload is 52 bytes
- channel state uses CAS transitions between `EMPTY`, `HAS_MSG`, and `CLOSED`
- current MRT workloads use:
  - forward channel: producer -> consumer
  - reverse channel: consumer -> producer alarm ACKs
- recv-to-user currently writes payload bytes directly from channel storage into EL0 memory to avoid prior copy corruption

### IO Mediator
- all user IO paths are mediated
- decisions are:
  - `Allow`
  - `Flag`
  - `Block`
- realtime processes are currently fast-pathed to `Allow`
- anomaly scores are tracked per PID and fed back into telemetry

### PAN Enforcement
- PAN support is detected at boot
- PAN is asserted directly on EL0 exception entry in the assembly entry stubs
- the kernel uses `ldtrb` / `sttrb` helpers for EL0 memory access from EL1
- this reduces accidental privileged dereference of user virtual addresses

## Runtime Telemetry

Important live telemetry forms include:

- `T,pid,intent,class,ns,anomaly` on core 0
- `T,pid,intent` on core 1
- `PPO,core,reward`
- MRT application logs such as:
  - `SND,<sensor>,<digit>`
  - `RCV,<count>`
  - `ACK`
  - `ALM,<sensor>`
  - `MRT`

## Security and Isolation Model

- userspace runs in EL0
- kernel runs in EL1
- process resources are capability-gated
- IO is mediated rather than directly trusted
- PAN is active on EL0 exception entry
- ASIDs are assigned per process to separate TLB state
- TTBR0 is switched per process during context switch

## Current MRT Dataflow

The current MRT demonstration pipeline is:

1. `mrt_producer` creates synthetic sensor readings.
2. The producer sends readings over the forward channel.
3. `mrt_consumer` receives and aggregates readings.
4. Readings with alarm flags trigger an ACK over the reverse channel.
5. The producer receives the ACK and prints `ALM,<sensor>`.

This demonstrates:

- bidirectional channel IPC
- realtime scheduling
- capability lookup from userspace
- mediated console output
- MAR telemetry
- PPO scheduler feedback in a live workload

## Known Limitations

- C workload MAR shim race still exists under SMP and remains a known limitation
- some legacy C workloads still fault under multicore execution and are recovered by terminating the faulting process
- PAC is unavailable in QEMU `virt`
- MTE is unavailable in QEMU
- there is no filesystem
- there is no real networking stack yet
- many core syscalls remain stubs or partial implementations
- console output can still interleave across cores because all logging targets a single UART

## Summary

Maya OS is a small, hardware-facing, AI-native kernel that combines:

- direct AArch64 control
- SMP scheduling
- capability-based isolation
- mediated IO
- bidirectional IPC
- MAR-based observability
- online scheduler adaptation

Its current architecture is intentionally experimental: it favors visibility, policy learning, and explicit control over broad device support or POSIX compatibility.

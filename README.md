# Maya OS

Maya is an AI-native operating system kernel built from scratch in Rust,
targeting both x86-64 and AArch64 bare metal.

Maya is not Linux with AI features bolted on. It is an operating system
designed from the ground up around the principle that the kernel itself
should be intelligent — capable of making scheduling decisions via a
neural network, gating every I/O operation through an AI anomaly detector,
transforming any legacy binary into an observable agentic process, and
speaking natural language.

Every scheduling decision is made by a neural network.
Every I/O operation is gated through an AI anomaly detector.
Every function call in a legacy binary is observable by the kernel AI.
The shell speaks English.

Maya boots on real x86-64 hardware and runs 8-core SMP on AArch64 via QEMU.

## Research

**Published paper:** https://doi.org/10.5281/zenodo.19218503
Target venue: OSDI

Author: Kolaparthi Jyothi Sarath, Independent Researcher

---

## Architecture

Maya is a capability-native kernel with an AI policy layer and an agentic
binary recompilation pipeline.

```
  ┌──────────────────────────────────────────┐
  │          Natural Language Shell           │
  │        (Qwen2.5-3B via Ollama)           │
  ├──────────────────────────────────────────┤
  │   MAR — Maya Agentic Recompiler          │
  │   Transforms ELF binaries into AEEs      │
  │   Every function call → Intent Bus       │
  ├──────────────────────────────────────────┤
  │  PPO Scheduler  │  I/O Mediator          │
  │  INT8 NEON MLP  │  Anomaly Scoring       │
  │  16-dim features│  IntentClass-weighted  │
  ├─────────────────┼────────────────────────┤
  │  Capability System  │  IPC               │
  │  128-bit tokens     │  WFE/SEV blocking  │
  │  PAC/MTE/BTI        │  SGI cross-core    │
  ├──────────────────────────────────────────┤
  │           Microkernel Core               │
  │  PMM  │  VMM  │  Exceptions  │  SMP      │
  └──────────────────────────────────────────┘
```

**Privilege model**: EL0/EL1 (AArch64) and Ring 0/3 (x86-64) with
capability-based access control. Every resource access requires an
unforgeable capability token.

**Memory**: Bitmap physical memory manager, 4-level page tables
(TTBR0/TTBR1 on AArch64, CR3 on x86-64), kernel heap.

**Security**: Capability tokens are cryptographically signed via
PACDA/AUTDA (AArch64 PAC) on hardware that supports it. Every I/O
operation is scored by an AI anomaly detector before execution.
BTI landing pads prevent ROP attacks. CSDB barriers mitigate
speculative execution attacks.

---

## The Maya Agentic Recompiler (MAR)

MAR is Maya's core innovation. It transforms any ELF binary — regardless
of source language — into an **Agentic Execution Environment (AEE)**.

Every function call in the transformed binary becomes:
- Observable by the kernel AI layer
- Routable via the Intent Bus
- Capability-gated
- Schedulable by the PPO policy

### How it works

**1. Binary Lifting**
MAR scans the binary for function prologues:
- x86-64: `push rbp; mov rbp, rsp` (5-byte E9 jump patch)
- AArch64: `stp x29, x30, [sp, #-N]!` (4-byte B instruction patch)

**2. Shim Injection**
Each function prologue is patched with a jump to a telemetry shim.
The shim saves registers, fires `SYS_TELEMETRY` (syscall 0x88), restores
registers, executes the original instructions, and returns.

**3. Output artifacts**
- `.mexe` — patched ELF with shim jumps at all function prologues
- `.mshm` — shim blob loaded at `0x04000000` (AArch64) / `0x7F000000` (x86-64)
- `.mlm` — JSON manifest with intent registry, IntentClass, capability requirements
- `.mlmb` — binary manifest for kernel loading (no JSON parser in kernel)

**4. Kernel integration**
At process spawn, the kernel:
- Maps `.mexe` segments and `.mshm` blob into the process address space
- Parses `.mlmb` to create `ResourceType::Intent` capabilities per function
- Registers each `(pid, intent_id) → CapToken` in the process intent table
- Notifies the PPO scheduler of the process's IntentClass

### Syscall Interface

```
SYS_INTENT_REG (0x80) — register intent name, returns CapToken (x0=lo, x1=hi)
SYS_TELEMETRY  (0x88) — fire telemetry event (hardcoded in shim blobs)
```

### Structured Telemetry Output

Every telemetry event emits a CSV line to serial:
```
T,<pid>,<intent_id>,<intent_class>,<tick_ns>,<anomaly_score>
```

Harvest with:
```bash
./scripts/run-aarch64.sh 2>&1 | grep "^T," > telemetry.csv
```

---

## Subsystems

### Capability System

128-bit unforgeable tokens on AArch64 (64-bit on x86-64).

**AArch64 token layout:**
```
[127:96] generation   (u32) — anti-replay counter
[95:80]  owner_pid    (u16)
[79:64]  rights       (u16) — READ|WRITE|EXECUTE|GRANT|REVOKE|INTENT_CALL|OBSERVE
[63:48]  resource_type(u16) — Memory|Channel|Process|Interrupt|Intent|Telemetry|Network|Crypto
[47:32]  intent_id    (u16) — MAR integration, embedded in token
[31:0]   slot_index   (u32)
```

Hardware exploitation (AArch64):
- **PACDA/AUTDA** — cryptographic token signing via hardware data key
- **MTE** — memory tag per capability slot, use-after-revoke faults at hardware level
- **BTI** — branch target identification on all entry points
- **CSDB** — speculation barrier after every validation branch
- **LSE CAS** — atomic generation counter update on revoke
- **PAN** — privileged access never, prevents EL1 from dereferencing EL0 pointers
- **GIC-v2 SGI** — per-core cache invalidation on revoke

Per-core capability cache (8 slots, lockless) eliminates table lock on hot path.

Resource types: Memory, Channel, Process, Interrupt, Intent, Telemetry, Network, Crypto

Rights: READ, WRITE, EXECUTE, GRANT, REVOKE, INTENT_CALL, OBSERVE

Delegation: parent_slot + delegation_depth + max_depth (default 3).

Fuzz suite: 17 tests — use-after-revoke, forgery, cross-owner, rights escalation,
double revoke, table recovery, generation wraparound, intent routing, delegation
depth limit, cache invalidation, MTE retag, PAC forgery.

### PPO Scheduler

3-layer INT8 MLP, 16-dimensional feature vector, 100Hz scheduling frequency.

**AArch64 hardware exploitation:**
- **NEON SIMD** — `vmull_s8`/`vaddlvq_s16` for 8× parallel INT8 MAC operations
- **CNTPCT_EL0** — nanosecond-resolution intent recency (feature slot 14)
- **WFE/SEV** — cores sleep between scheduling events, woken by SEV on process add
- **MPIDR_EL1** — hardware core ID, no software counter needed
- Per-core `ModelWeights` — lockless, zero contention across 8 cores

**16-dim feature vector:**

| Slot | Feature | MAR integration |
|------|---------|-----------------|
| 0 | process class | — |
| 1 | CPU usage % | — |
| 2 | wait time | — |
| 3 | I/O wait ratio | — |
| 4 | IPC send rate | — |
| 5 | IPC recv rate | — |
| 6 | memory pages | — |
| 7 | page fault rate | — |
| 8 | priority hint | — |
| 9 | time since input | — |
| 10 | burst length | — |
| 11 | deadline urgency | — |
| 12 | **intent_weight** | **MAR: IntentClass → PPO** |
| 13 | starvation risk | — |
| 14 | **intent_recency** | **MAR: nanosecond CNTPCT_EL0** |
| 15 | cap count | — |

IntentClass weights: Compute=1.0, RealTime=0.95, IO=0.7, System=0.6, Background=0.1

### I/O Mediator

Intercepts every file, memory, network, and IPC operation.
Scores each request using 4 anomaly rules with IntentClass-weighted discounts.

**AArch64 hardware exploitation:**
- **CNTPCT_EL0** — true nanosecond latency measurement per mediation decision
- **DSB/ISB** — memory barriers for MemoryMap requests
- **RawSpinLock** — LDAXR/STLXR, no external crate dependency
- **DMB ST** — store barrier before marking scope entries valid

**AI-native features:**
- Capability check before anomaly scoring (no cap = immediate Block)
- IntentClass discount: IO-class processes score lower suspicion for network ops
- Audit → PPO feedback: Block/Flag decisions call `update_process_intent()`
- Fixed-size scope table (16 entries, no BTreeMap) and circular history buffer (64 entries)
- True nanosecond latency in every `IoEvent` audit record

Anomaly rules:
1. Batch process flood (>50 file opens in 500ms window)
2. Cross-process /proc access
3. Out-of-scope file write
4. Repeated identical requests (>20 in 500ms window)

Decision: score=0.0 → Allow, score<0.5 → Flag, score≥0.5 → Block

Fuzz suite: 8 tests including intent_class_discount and audit→scheduler feedback.

### IPC

Synchronous message passing with capability transfer.

**Message structure (AArch64):**
```rust
pub struct Message {
    pub sender_pid:   u16,
    pub intent_id:    u16,        // MAR intent context
    pub intent_class: IntentClass,// PPO scheduler hint
    pub payload:      [u8; 52],
    pub cap_transfer: Option<CapToken>,
}
```

**AArch64 hardware exploitation:**
- **LSE CAS** — atomic channel state transitions (Empty↔HasMessage↔Closed)
- **DSB ST + SEV** — payload visible before wake signal fires
- **WFE** — blocking recv sleeps core until sender's SEV
- **GIC-v2 SGI #1** — cross-core IPC notification (SGI #0 reserved for cap cache)
- Fixed-size channel table (64 channels, no Vec)

Capability transfer validated via `cap::delegate()` — receiver gets a properly
owned token, not a copy of the sender's token.

Scheduler notification on every send — PPO feature vector updated immediately.

Fuzz suite: 10 tests.

### Userspace ABI (AArch64)

Native AArch64 syscall ABI — not a port of x86-64.

**Register convention:**
```
x8  = syscall number
x0  = arg0 / CapToken (required for 0x10+, 0x100+ ranges)
x1  = arg1 / CapToken high word (for 128-bit token returns)
x2-x7 = further arguments
x0  = return value
x1  = return high word (CapToken returns)
```

**Syscall table:**
```
0x00  SYS_EXIT         0x20  SYS_CHAN_CREATE
0x01  SYS_YIELD        0x21  SYS_CHAN_SEND
0x02  SYS_SPAWN        0x22  SYS_CHAN_RECV
0x03  SYS_GETCAP       0x23  SYS_CHAN_RECV_NB
0x04  SYS_QUERY
                       0x80  SYS_INTENT_REG  → returns CapToken
0x10  SYS_CAP_CREATE   0x88  SYS_TELEMETRY   (MAR shim hardcoded)
0x11  SYS_CAP_REVOKE
0x12  SYS_CAP_GRANT    0x100 SYS_READ        (auto-mediated)
0x13  SYS_CAP_CHECK    0x101 SYS_WRITE       (auto-mediated)
                       0x102 SYS_OPEN        (auto-mediated)
                       0x103 SYS_NET_SEND    (auto-mediated)
                       0x104 SYS_NET_RECV    (auto-mediated)
                       0x105 SYS_MMAP        (auto-mediated)
```

All 0x100+ syscalls automatically invoke the I/O mediator before dispatch.
`SYS_INTENT_REG` returns a 128-bit `ResourceType::Intent` CapToken split
across x0 (lo) and x1 (hi).

**EL0/EL1 transition:**
- SVC entry: full GPR save (x0-x30 + ELR + SPSR) via vectors.s
- User pointer reads: `LDTRB` (unprivileged load, PAN respected)
- Return to EL0: SPSR_EL1=0 (EL0t, interrupts enabled), ERET
- Per-process TTBR0_EL1 with ASID for TLB isolation

### VMM (AArch64)

4-level page tables (L0→L3), 4KB pages, 48-bit VA via TCR T0SZ=16.

- `alloc_user_table()` — allocates L0 root from PMM, zeroed
- `map_user_segment()` — walks/creates page tables, copies data, DSB+ISB
- `set_user_table()` — sets TTBR0_EL1 with ASID, TLB invalidate
- PTE flags: AF, SH_IS, AP_RW_USER/RO_USER, UXN, PXN, ATTRINDX_NORMAL

### SMP

**x86-64:** ACPI MADT topology detection. AP startup via 16-bit trampoline.
Note: EDK2 firmware blocks AP startup on some machines; 1 core verified on Dell Inspiron.

**AArch64:** 8 cores online via PSCI CPU_ON. Verified on QEMU virt with GIC-v2.
Each AP: FP/SIMD enabled (CPACR_EL1.FPEN=0b11), Generic Timer at 100Hz,
per-core scheduler queue initialized, WFE idle loop.

### Natural Language Shell

Accepts command shortcuts and natural language queries.
Routes complex queries to Qwen2.5-3B via Ollama bridge.
Maintains semantic context across conversation turns.

Python bridge: `scripts/maya-bridge.py` connects Maya serial UART to
local Ollama instance.

---

## Performance

Measured on Dell Inspiron (Intel Core, real hardware, x86-64):

| Subsystem | Maya | Linux equiv |
|-----------|------|-------------|
| IPC round-trip | 328 cycles | ~800 cycles |
| Capability lifecycle | 144 cycles | N/A |
| I/O mediation decision | 1,650 cycles | N/A |
| AI scheduler score | 41,021 cycles | N/A |
| PMM alloc+free | 2,037 cycles | ~200 cycles |

Maya IPC is 2× faster than Linux pipe IPC — IPC is a first-class
kernel primitive with no Unix socket overhead.

The I/O mediator adds ~550ns of AI security overhead per I/O operation.
Every file access, memory operation, and network call is AI-gated.

AArch64 NEON inference is 8× faster than scalar for the INT8 MLP
forward pass (verified via vmull_s8/vaddlvq_s16 NEON path).

---

## Building

### Prerequisites

```bash
# Rust nightly
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly

# QEMU
brew install qemu

# AArch64 cross-assembler (for MAR test binary)
brew install aarch64-elf-binutils

# Python deps for MAR tool
source tools/mar/venv/bin/activate
# capstone and pyelftools already installed in venv
```

### x86-64 kernel

```bash
cargo +nightly rustc \
  -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins \
  -Zbuild-std-features=compiler-builtins-mem \
  --target targets/x86_64-aios.json \
  -p kernel --bin kernel --offline
```

### AArch64 kernel

```bash
./scripts/run-aarch64.sh
```

### MAR tool (AArch64)

```bash
source tools/mar/venv/bin/activate
python3 tools/mar/mar_aarch64.py <path/to/elf>
```

Output: `<binary>.mexe`, `<binary>.mshm`, `<binary>.mlm`, `<binary>.mlmb`

### MAR tool (x86-64)

```bash
source tools/mar/venv/bin/activate
python3 tools/mar/mar.py <path/to/elf>
```

---

## Running

### AArch64 (QEMU virt, 8 cores)

```bash
./scripts/run-aarch64.sh
```

Expected boot output:
```
Maya AArch64 booting...
PMM initialised
Exception vectors installed
MMU configured
GIC-v2 initialised
Timer initialised at 100Hz
Scheduler: init complete
Process table initialised
[8 cores online]
Maya AArch64 ready
cap fuzz: 17 passed, 0 failed
IO fuzz: 8 passed, 0 failed
IPC fuzz: 10 passed, 0 failed
MAR process launched pid=4
maya-arm>
```

### x86-64 (QEMU with UEFI)

```bash
qemu-system-x86_64 \
  -drive if=pflash,format=raw,readonly=on,\
file=/opt/homebrew/share/qemu/edk2-x86_64-code.fd \
  -drive format=raw,file=target/x86_64-aios/debug/uefi.img \
  -serial stdio -m 256M -no-reboot -smp 4
```

### AI shell bridge

```bash
# Terminal 1: QEMU with PTY serial
qemu-system-x86_64 ... -serial pty -display none

# Terminal 2: NL bridge
source scripts/venv/bin/activate
python3 scripts/maya-bridge.py /dev/ttysXXX
```

### Telemetry harvest

```bash
./scripts/run-aarch64.sh 2>&1 | grep "^T," > telemetry.csv
```

CSV columns: `type, pid, intent_id, intent_class, tick_ns, anomaly_score`

### Boot on real hardware (x86-64)

```bash
bash scripts/build-usb.sh /dev/diskN
```

Verified on: Dell Inspiron (Intel Core, 8 logical cores, 16GB RAM).

---

## Project Structure

```
maya/
├── Cargo.toml                  workspace root
├── targets/
│   ├── x86_64-aios.json        x86-64 custom target
│   └── aarch64-maya.json       AArch64 custom target
├── crates/
│   ├── kernel/src/             x86-64 kernel (Phase 0-9 complete)
│   │   ├── arch/               GDT, IDT, SMP, APIC
│   │   ├── memory/             PMM, VMM, heap
│   │   ├── cap/                64-bit capability system
│   │   ├── ipc/                synchronous message passing
│   │   ├── io/                 I/O mediator, syscalls, audit
│   │   ├── sched/              PPO scheduler, process model
│   │   ├── model/              INT8 MLP weights, inference
│   │   ├── proc/               ELF loader, process table, MAR launch
│   │   ├── shell/              NL shell, intent parser
│   │   └── fs/                 VFS, memfs
│   └── kernel-aarch64/src/     AArch64 kernel (Phase 10 complete)
│       ├── arch/               exceptions, GIC-v2, MMU, timer, PSCI, CPU
│       ├── memory/             PMM, VMM (TTBR0 page tables)
│       ├── cap/                128-bit capability system (PAC/MTE/BTI)
│       ├── ipc/                WFE/SEV IPC with SGI cross-core
│       ├── io/                 I/O mediator (CNTPCT_EL0 latency)
│       ├── sched/              PPO scheduler (NEON MLP)
│       ├── model/              INT8 MLP, NEON inference
│       └── proc/               ELF loader, syscalls, MLM parser, MAR launch
├── tools/mar/
│   ├── mar.py                  MAR tool — x86-64
│   └── mar_aarch64.py          MAR tool — AArch64
├── userspace/
│   ├── hello_c/                C hello world + .mexe (x86-64)
│   ├── hello_rust/             Rust no_std hello world
│   └── hello_aarch64/          AArch64 test binary + .mexe/.mshm/.mlmb
├── paper/
│   └── maya-osdi.tex           Research paper (832 lines)
└── scripts/
    ├── run-aarch64.sh          AArch64 QEMU boot script
    ├── maya-bridge.py          Ollama NL bridge
    └── build-usb.sh            x86-64 USB image builder
```

---

## Roadmap

### Completed

**Phase 0-9 — x86-64 kernel**
- PMM, VMM, GDT, IDT, SMP
- Capability system (64-bit tokens, generation counters)
- IPC (56-byte payloads, capability transfer)
- PPO scheduler (INT8 MLP, 16-dim features, 100Hz)
- I/O mediator (4 anomaly rules, audit log)
- ELF loader (ET_EXEC + ET_DYN/PIE)
- Natural language shell (Qwen2.5-3B via Ollama)
- MAR x86-64 (E9 jump patching, telemetry shims)
- Userspace programs (C, Rust, Assembly)

**Phase 10 — AArch64 port (native, not a port)**
- 8-core SMP via PSCI, GIC-v2, Generic Timer
- 128-bit capability system with PAC/MTE/BTI/LSE
- PPO scheduler with NEON MLP inference
- I/O mediator with CNTPCT_EL0 nanosecond latency
- IPC with WFE/SEV blocking and SGI cross-core
- Native AArch64 syscall ABI (x8=nr, SVC, ERET)
- VMM with TTBR0/ASID per-process page tables
- ELF loader for AArch64 userspace
- MAR AArch64 (B instruction patching, stp x29,x30 prologue scanning)
- Structured telemetry CSV output for training data harvest

### Next

**Phase 11 — Context switching + PPO weight training**
- Full EL0/EL1 context switch (save/restore all registers)
- Real telemetry harvest from running processes
- Python gym environment (OpenAI Gym) mimicking Maya 100Hz tick
- PPO weight training on M4 via MLX (Apple Neural Engine)
- Trained `.bin` weights embedded in AArch64 kernel
- Benchmark: Maya PPO vs Linux CFS on mixed workloads

**Phase 12 — Network + storage**
- VirtIO network driver (AArch64)
- ext2 / in-memory VFS (AArch64)
- Full process lifecycle (fork/exec/wait)

**Phase 13 — Hardware**
- Boot on real AArch64 hardware (Raspberry Pi 5 / Graviton3)
- PAC hardware validation (ARMv8.3+)
- MTE hardware validation (ARMv8.5+)
- OSDI paper submission with trained PPO results
# maya

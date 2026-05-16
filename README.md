# Maya OS

**An AI-native operating system for AArch64.**

Maya is a bare-metal operating system built from scratch for 64-bit ARM hardware. It is not a Linux distribution, not a POSIX clone, and not a research toy. Every primitive — the scheduler, the filesystem, the IPC system, the memory model — is designed around the assumption that the OS itself is an intelligent agent, not a passive resource allocator.

---

## What Makes Maya Different

Most operating systems treat AI as an application. Maya treats AI as infrastructure.

**The PPO Scheduler** learns at runtime. Every scheduling decision produces a reward signal. Every reward updates a policy gradient model running inside the kernel. The scheduler that boots is not the scheduler that runs an hour later — it has adapted to the actual workload on your hardware.

**MAR (Maya Agentic Runtime)** wraps every userspace function call with observable intent metadata. Every operation is capability-gated, classified by intent class, and fed into the PPO reward pipeline. Any app built on MRT — in any language — is automatically observable and schedulable by intent, not just by priority number.

**MayaFS** is a content-addressed, versioned, semantically tagged filesystem. Every file has a version history. Files can be delegated, revoked, and queried by semantic intent. The filesystem is AI-native: it understands what data is, not just where it is stored.

**The AI Intent Interface** lets you query the kernel in natural language. Type `? why is core 2 getting high rewards` in the shell and the embedded AI reasons about live telemetry and answers. Every query is logged as training data for APEX, Maya's purpose-built inference model.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Maya OS                              │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                   AArch64 Kernel                    │   │
│  │                                                     │   │
│  │  PPO Scheduler    MayaFS         IPC / Capabilities │   │
│  │  (online learn)   (versioned,    (capability-gated, │   │
│  │                   content-addr)   alarm/ack proto)  │   │
│  │                                                     │   │
│  │  GIC-v2  LSE CAS  WFE/SEV  NEON  PAN  CNTPCT_EL0  │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              MRT (Maya Runtime) — Userspace         │   │
│  │                                                     │   │
│  │  11 processes across 8 cores                        │   │
│  │  Intent classes: RealTime / Compute / IO / BG       │   │
│  │  MAR shims on every function call                   │   │
│  └─────────────────────────────────────────────────────┘   │
│                                                             │
│  ┌─────────────────────────────────────────────────────┐   │
│  │           AI Inference Layer (Pluggable)            │   │
│  │                                                     │   │
│  │  Today: Qwen2.5-3B via mlx-lm (local, offline)     │   │
│  │  Next:  APEX — purpose-built model for Maya        │   │
│  │  Interface: MAYA_AI_BACKEND=qwen|apex|claude        │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
```

---

## Hardware

| Target | Status |
|--------|--------|
| QEMU virt (AArch64) | ✅ Primary development platform |
| Apple M4 (host) | ✅ Development + Qwen inference |
| Raspberry Pi 4 | 🔄 Planned — Phase 30 |
| Hailo-8 NPU | 🔄 Planned — dedicated inference silicon |

**QEMU flags:** `-machine virt,gic-version=2 -cpu max,pauth=on -smp 8 -m 256M`

---

## What Is Working

| Subsystem | Detail |
|-----------|--------|
| AArch64 boot | Bare metal, no bootloader dependency |
| 8-core SMP | PSCI, per-core stacks, SGI wakeup |
| MMU | 4KB pages, kernel/user split |
| GIC-v2 | Full interrupt controller |
| LSE atomics | CASAL for all shared state |
| WFE/SEV | Core sleep/wake primitives |
| NEON | Vector operations |
| PAN | Privileged Access Never enforced |
| PPO Scheduler | Online learning, weights adapting at runtime |
| MAR shims | All MRT functions observable |
| Capability system | READ/WRITE/GRANT, delegation, revocation |
| IPC channels | Bidirectional, alarm/ack protocol, ALM/ACK tracked |
| MayaFS | Versioning, /proc, semantic tags, capability delegation |
| virtio-net | UDP/IP/Ethernet, SYS_NET_SEND/RECV/MAC |
| virtio-gpu | Framebuffer (compiled, headless in current config) |
| virtio-keyboard | HID keycode translation, KEY_RING buffer |
| MRT shell | 14 commands, clean output isolation |
| Telemetry | UART → TCP → WebSocket → browser, ~500ms cadence |
| Browser HUD | Live process topology, swarm animation, shell panel |
| AI Intent Interface | `?` queries answered by Qwen2.5 from live telemetry |
| Training pipeline | Every `?` query logged to JSONL for APEX fine-tuning |

---

## Two Cardinal Principles

These are never compromised, in any phase:

**P1 — Full AArch64 hardware exploitation**
PAC, LSE CAS, WFE/SEV, MPIDR_EL1, GIC-v2, CNTPCT_EL0, NEON, PAN. Maya does not abstract away the hardware — it uses every capability the ISA provides.

**P2 — AI-native / MAR**
Every operation is observable intent, capability-gated, and PPO-scheduled. The kernel is not a passive resource manager. It is an online learning agent that gets smarter the longer it runs.

---

## Memory Map

| Region | Physical Address |
|--------|-----------------|
| Kernel | `0x40200000` |
| ELF slots | `0x41800000 + slot × 0x100000` |
| Stacks | `0x50000000 + slot × 0x10000` |
| MAR Shims | `0x47F00000 + slot × 0x10000` |
| GPU framebuffer | `0x48000000` (4MB) |
| VirtQueue mem | `0x46000000` |

---

## Process Table

| PID | Name | Core | Intent |
|-----|------|------|--------|
| 4 | compute_workload | 2 | Compute |
| 5 | io_workload | 3 | IO |
| 6 | background_task | 4 | Background |
| 7 | matrix_multiply | 5 | Compute |
| 8 | net_parser | 6 | IO |
| 9 | sort_suite | 7 | Background |
| 10 | mrt_hello | 1 | RealTime |
| 11 | mrt_producer | 0 | RealTime |
| 12 | mrt_consumer | 0 | RealTime |
| 13 | mrt_logger | 1 | IO |
| 14 | mrt_shell | 1 | IO |

---

## Running Maya

### Requirements

- macOS (Apple Silicon recommended) or Linux
- QEMU 8+ with AArch64 support: `brew install qemu`
- Rust nightly with AArch64 bare-metal target
- Python 3.11+
- For AI intent interface: `pip install mlx-lm` + Qwen2.5-3B model

### Boot

```bash
# Install dependencies
brew install qemu
rustup target add aarch64-unknown-none

# Boot Maya (builds kernel, starts bridge, opens HUD)
chmod +x scripts/run-aarch64.sh
./scripts/run-aarch64.sh

# Open the HUD in your browser
open scripts/maya-hud.html
```

### AI Intent Interface

```bash
# Set inference backend
export MAYA_AI_BACKEND=qwen          # local Qwen2.5 (default)
# export MAYA_AI_BACKEND=apex        # APEX (when trained)
# export MAYA_AI_BACKEND=claude      # Claude API fallback

# In the HUD shell, press / to focus, then:
? why is core 2 getting high rewards
? what is the current state of mayafs
? which process is sending the most alarms
```

### Switching AI Backends

```bash
# Qwen2.5 via mlx-lm (fastest on Apple Silicon)
export MAYA_AI_BACKEND=qwen
export MAYA_QWEN_MODEL=~/models/qwen2.5-3b-mlx

# APEX (when APEX MEDIUM is trained and served)
export MAYA_AI_BACKEND=apex
export MAYA_APEX_ENDPOINT=http://localhost:8080/v1/generate

# Claude API
export MAYA_AI_BACKEND=claude
export ANTHROPIC_API_KEY=sk-ant-...
```

---

## APEX — Maya's Purpose-Built AI

APEX (Adaptive Polymorphic Execution Engine) is a decoder-only hybrid architecture interleaving Mamba SSM blocks with transformer attention. It is being trained specifically for Maya kernel reasoning.

Key properties relevant to Maya:
- **TurboQuant KV cache** — bounded memory usage regardless of session length, critical for a long-running OS inference daemon
- **Continuous Latent Reasoning (RSM)** — private reasoning steps per output token, never decoded
- **MoE architecture** — ~125M active parameters from ~1B total at MEDIUM tier, efficient for embedded inference
- **Apache 2.0 license** — ships with Maya without restriction

APEX replaces Qwen as a drop-in via `MAYA_AI_BACKEND=apex`. Every `?` query answered by Qwen today is logged to `scripts/maya-training-data.jsonl` and becomes fine-tuning data for APEX MEDIUM.

---

## Codebase Structure

```
crates/
├── kernel-aarch64/     ← The OS kernel
│   ├── src/arch/       ← Boot, MMU, GIC, vectors, timer
│   ├── src/sched/      ← PPO scheduler, process queue
│   ├── src/ipc/        ← Capability-gated channels
│   ├── src/fs/         ← MayaFS store, namespace, tags
│   ├── src/net/        ← virtio-net, UDP stack
│   ├── src/gpu/        ← virtio-gpu, canvas
│   ├── src/input/      ← virtio-keyboard
│   ├── src/model/      ← PPO weights, inference
│   └── src/telemetry.rs
├── mrt/                ← Maya Runtime (userspace API)
└── kernel/             ← x86-64 prototype (archived)

userspace/
├── mrt_producer/       ← RealTime sensor producer
├── mrt_consumer/       ← RealTime sensor consumer
├── mrt_hello/          ← RealTime alarm handler
├── mrt_logger/         ← IO logger
├── mrt_shell/          ← Interactive shell (pid 14)
├── compute_workload/   ← Compute intent workload
├── matrix_multiply/    ← Compute intent workload
├── io_workload/        ← IO intent workload
├── net_parser/         ← IO intent / network
├── background_task/    ← Background intent
└── sort_suite/         ← Background intent

scripts/
├── run-aarch64.sh      ← Boot script (QEMU + bridge)
├── maya-bridge.py      ← TCP:4444 ↔ WebSocket:8765
├── maya-hud.html       ← Browser HUD
└── maya-training-data.jsonl  ← APEX training corpus
```

---

## Git Tags

```
maya-v1.0           Phase 1–20: Foundation complete
maya-v2.0           Phase 25: Shell + premium HUD
maya-v2.1-pre-apex  Phase 27: PPO adapting, ALM/ACK live
maya-v3.0           Phase 28: Qwen intent interface, pluggable AI backend
```

---

## Roadmap

| Phase | Description | Status |
|-------|-------------|--------|
| 1–20 | AArch64 boot, MMU, SMP, MAR, PPO, MayaFS, virtio | ✅ Complete |
| 21–25 | Stability, shell, premium HUD | ✅ Complete |
| 26–27 | Shell isolation, topology redesign, PPO fix, ALM/ACK | ✅ Complete |
| 28 | AI intent interface, Qwen2.5, pluggable backend | ✅ Complete |
| 29 | MAR semantic enrichment — Qwen enriches PPO rewards | 🔄 Next |
| 30 | Raspberry Pi 4 port — real PAC, real hardware | 🔄 Planned |
| 31 | APEX MEDIUM fine-tuned on Maya telemetry | 🔄 Planned |
| 32 | Hailo-8 NPU integration — dedicated inference silicon | 🔄 Planned |

---

## Design Philosophy

Maya's interface is not a bash clone. It reflects the kernel's actual nature.

**Intent-first** — commands express what you want, not what binary to execute.

**Everything observable** — the orbital swarm in the HUD IS the scheduler. The spokes ARE reward signals. The red nodes ARE active IPC. Nothing is hidden.

**No hidden intelligence** — the PPO weights, reward history, process topology, and AI reasoning are all visible and queryable.

**One window** — the browser IS Maya. QEMU runs headless. There is no separate terminal, no window manager, no desktop. The HUD is the computer.

---

## Contributing

Maya is under active development. The codebase is a single collaborator + AI pair programming project. If you want to contribute, open an issue describing what you want to build.

Areas where contributions would be most useful:
- Raspberry Pi 4 MMIO port
- Additional MRT process types
- APEX inference server implementation
- Real hardware PAC testing

---

*Maya OS — the kernel that learns.*
#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

echo "Building AArch64 kernel..."
cargo +nightly build -Zjson-target-spec \
  -Zbuild-std=core,alloc,compiler_builtins \
  -Zbuild-std-features=compiler-builtins-mem \
  --target targets/aarch64-maya.json \
  -p kernel-aarch64 \
  --offline

echo "Extracting raw binary..."
python3 - << 'PYEOF'
import struct
with open('target/aarch64-maya/debug/kernel-aarch64', 'rb') as f:
    elf = f.read()
e_phoff = struct.unpack_from('<Q', elf, 32)[0]
e_phnum = struct.unpack_from('<H', elf, 56)[0]
e_phentsize = struct.unpack_from('<H', elf, 54)[0]
load_addr = 0xFFFFFFFFFFFFFFFF
end_addr = 0
segments = []
for i in range(e_phnum):
    off = e_phoff + i * e_phentsize
    p_type = struct.unpack_from('<I', elf, off)[0]
    p_offset = struct.unpack_from('<Q', elf, off + 8)[0]
    p_vaddr = struct.unpack_from('<Q', elf, off + 16)[0]
    p_paddr = struct.unpack_from('<Q', elf, off + 24)[0]
    p_filesz = struct.unpack_from('<Q', elf, off + 32)[0]
    p_memsz = struct.unpack_from('<Q', elf, off + 40)[0]
    if p_type == 1 and p_filesz > 0:
        load_addr_seg = p_paddr if p_paddr != 0 else p_vaddr
        segments.append((load_addr_seg, p_offset, p_filesz))
        load_addr = min(load_addr, load_addr_seg)
        end_addr = max(end_addr, load_addr_seg + p_memsz)
raw = bytearray(end_addr - load_addr)
for paddr, offset, filesz in segments:
    raw[paddr - load_addr:paddr - load_addr + filesz] = elf[offset:offset + filesz]
with open('target/aarch64-maya/debug/kernel.bin', 'wb') as f:
    f.write(raw)
print(f'Binary: {len(raw)} bytes at 0x{load_addr:x}')
PYEOF

cleanup() {
  if [[ -n "${BRIDGE_PID:-}" ]]; then
    kill "${BRIDGE_PID}" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

echo "Starting Maya Telemetry Bridge..."
BRIDGE_BACKEND="${MAYA_AI_BACKEND:-qwen}"
BRIDGE_PYTHON="python3"
if [[ "${BRIDGE_BACKEND}" != "qwen" ]]; then
  if [[ -x "tools/mar/venv/bin/python" ]]; then
    BRIDGE_PYTHON="tools/mar/venv/bin/python"
  elif [[ -x "tools/mar/venv/bin/python3" ]]; then
    BRIDGE_PYTHON="tools/mar/venv/bin/python3"
  fi
fi
"${BRIDGE_PYTHON}" scripts/maya-bridge.py &
BRIDGE_PID=$!
sleep 0.6

echo "Booting Maya AArch64..."
run_qemu() {
  qemu-system-aarch64 \
    -machine virt,gic-version=2 \
    -global virtio-mmio.force-legacy=off \
    -cpu max,pauth=on \
    -smp 8 \
    -m 256M \
    "$@" \
    -device loader,\
file=target/aarch64-maya/debug/kernel.bin,\
addr=0x40200000,cpu-num=0 \
    -serial tcp::4444,server,nowait \
    -no-reboot
}

if ! run_qemu \
  -netdev user,id=net0,hostfwd=udp::5555-:5555 \
  -device virtio-net-device,netdev=net0; then
  echo "Retrying without host UDP forwarding..."
  run_qemu \
    -netdev user,id=net0 \
    -device virtio-net-device,netdev=net0
fi

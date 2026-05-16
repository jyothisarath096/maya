#!/usr/bin/env python3

import json
import os
import struct
import sys

try:
    from capstone import CS_ARCH_ARM64, CS_MODE_ARM, Cs
    from elftools.elf.elffile import ELFFile
except ImportError as exc:
    raise SystemExit(
        "mar_aarch64.py requires the Python packages 'capstone' and 'pyelftools': {}".format(exc)
    )


PT_LOAD = 1
PF_X = 1
PATCH_LEN = 4
SAVE_LEN = 4
INTENT_BASE = 100
ELF_SLOT_BASE = 0x41800000
ELF_SLOT_STRIDE = 0x00100000
SCRATCH_BASE = 0x47FE0000
SCRATCH_STRIDE = 0x1000
SHIM_LAYOUTS = {
    "compute_workload": (0x47F00000, 0x47FFF000, "Compute"),
    "io_workload": (0x47F10000, 0x48000000, "IO"),
    "background_task": (0x47F20000, 0x48001000, "Background"),
    "matrix_multiply": (0x47F30000, 0x47F3F000, "Compute"),
    "net_parser": (0x47F40000, 0x47F4F000, "IO"),
    "sort_suite": (0x47F50000, 0x47F5F000, "Compute"),
    "mrt_hello": (0x47F60000, 0x47F6F000, "Compute"),
    "mrt_producer": (0x47F70000, 0x47F7F000, "Compute"),
    "mrt_consumer": (0x47F80000, 0x47F8F000, "IO"),
    "mrt_shell": (0x47F90000, 0x47F9F000, "IO"),
    "hello_aarch64": (0x47F00000, 0x47FFF000, "Unknown"),
}
NO_HOOK_APPS = {"mrt_shell"}
SKIP_HOOK_SYMBOLS = {
    "mrt_producer": {"_start"},
    "mrt_consumer": {"_start"},
    "mrt_shell": {"_start"},
}
MAX_ANALYZE_BYTES = 32
MLMB_VERSION = 4
DEFAULT_CAP_BITMAP = 0x0060


def fnv1a_64(text: str) -> int:
    value = 0xCBF29CE484222325
    for byte in text.encode("utf-8"):
        value ^= byte
        value = (value * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return value


def file_offset_for_vaddr(segments, vaddr):
    for seg in segments:
        start = seg["vaddr"]
        end = start + seg["filesz"]
        if start <= vaddr < end:
            return seg["offset"] + (vaddr - start)
    return None


def is_prologue(word0, word1):
    if (word0 & 0xFFC07FFF) == 0xA9807BFD:
        return True
    if word0 == 0xD503245F and (word1 & 0xFFC07FFF) == 0xA9807BFD:
        return True
    if word0 == 0xD503233F and (word1 & 0xFFC07FFF) == 0xA9807BFD:
        return True
    if (word0 & 0xFFC07FFF) == 0xA9807A73:
        return True
    return False


def discover_functions(elffile):
    functions = {}
    exec_segments = []

    for section in elffile.iter_sections():
        if section["sh_type"] not in ("SHT_SYMTAB", "SHT_DYNSYM"):
            continue
        for symbol in section.iter_symbols():
            if symbol["st_info"]["type"] != "STT_FUNC":
                continue
            vaddr = symbol["st_value"]
            if vaddr == 0:
                continue
            name = symbol.name or "sub_{:x}".format(vaddr)
            functions[vaddr] = {"name": name, "size": symbol["st_size"]}

    for segment in elffile.iter_segments():
        if segment["p_type"] != "PT_LOAD" or (segment["p_flags"] & PF_X) == 0:
            continue
        exec_segments.append(
            {
                "offset": segment["p_offset"],
                "vaddr": segment["p_vaddr"],
                "filesz": segment["p_filesz"],
                "memsz": segment["p_memsz"],
                "flags": segment["p_flags"],
                "data": segment.data(),
            }
        )

    for seg in exec_segments:
        data = seg["data"]
        for pos in range(0, max(0, len(data) - 7), 4):
            word0 = struct.unpack_from("<I", data, pos)[0]
            word1 = struct.unpack_from("<I", data, pos + 4)[0]
            if is_prologue(word0, word1):
                vaddr = seg["vaddr"] + pos
                functions.setdefault(vaddr, {"name": "sub_{:x}".format(vaddr), "size": 0})

    ordered = []
    for vaddr in sorted(functions):
        ordered.append(
            {"vaddr": vaddr, "name": functions[vaddr]["name"], "size": functions[vaddr]["size"]}
        )
    return ordered, exec_segments


def analyze_patch_window(binary, file_offset, func_vaddr):
    md = Cs(CS_ARCH_ARM64, CS_MODE_ARM)
    md.detail = False
    code = bytes(binary[file_offset:file_offset + MAX_ANALYZE_BYTES])

    for insn in md.disasm(code, func_vaddr):
        mnemonic = insn.mnemonic.lower()
        if mnemonic in {"b", "bl", "cbz", "cbnz", "tbz", "tbnz", "adr", "adrp"} and insn.address == func_vaddr:
            return None, "relative control-flow opcode at entry ({})".format(mnemonic)
        if insn.size != SAVE_LEN:
            return None, "unexpected AArch64 instruction size at entry ({})".format(insn.size)
        return {
            "save_len": SAVE_LEN,
            "saved_bytes": bytes(insn.bytes),
            "instructions": [
                {
                    "address": insn.address,
                    "size": insn.size,
                    "mnemonic": insn.mnemonic,
                    "op_str": insn.op_str,
                }
            ],
        }, None

    return None, "could not decode complete AArch64 entry instruction"


def encode_u32(word):
    return struct.pack("<I", word & 0xFFFFFFFF)


def encode_b(source_va, target_va):
    offset = target_va - source_va
    if offset % 4 != 0:
        raise ValueError("branch target must be 4-byte aligned")
    signed_imm26 = offset >> 2
    if not (-(1 << 25) <= signed_imm26 < (1 << 25)):
        raise ValueError("branch out of range")
    return encode_u32(0x14000000 | (signed_imm26 & 0x03FFFFFF))


def encode_abs_branch(target_va, scratch_reg=10):
    stub = bytearray()
    stub += encode_movz(scratch_reg, target_va & 0xFFFF)
    if target_va > 0xFFFF:
        stub += encode_movk(scratch_reg, (target_va >> 16) & 0xFFFF, 16)
    if target_va > 0xFFFFFFFF:
        stub += encode_movk(scratch_reg, (target_va >> 32) & 0xFFFF, 32)
    if target_va > 0xFFFFFFFFFFFF:
        stub += encode_movk(scratch_reg, (target_va >> 48) & 0xFFFF, 48)
    stub += encode_u32(0xD61F0000 | (scratch_reg << 5))
    return bytes(stub)


def encode_stp_pre(rt, rt2, rn, imm):
    if imm % 8 != 0:
        raise ValueError("stp pre-index immediate must be 8-byte aligned")
    imm7 = (imm // 8) & 0x7F
    return encode_u32(0xA9800000 | (1 << 24) | (imm7 << 15) | (rt2 << 10) | (rn << 5) | rt)


def encode_stp_off(rt, rt2, rn, imm):
    imm7 = (imm // 8) & 0x7F
    return encode_u32(0xA9000000 | (imm7 << 15) | (rt2 << 10) | (rn << 5) | rt)


def encode_ldp_off(rt, rt2, rn, imm):
    imm7 = (imm // 8) & 0x7F
    return encode_u32(0xA9400000 | (imm7 << 15) | (rt2 << 10) | (rn << 5) | rt)


def encode_ldp_post(rt, rt2, rn, imm):
    imm7 = (imm // 8) & 0x7F
    return encode_u32(0xA8C00000 | (imm7 << 15) | (rt2 << 10) | (rn << 5) | rt)


def encode_str_pre(rt, rn, imm):
    imm9 = imm & 0x1FF
    return encode_u32(0xF8000C00 | (imm9 << 12) | (rn << 5) | rt)


def encode_ldr_post(rt, rn, imm):
    imm9 = imm & 0x1FF
    return encode_u32(0xF8400400 | (imm9 << 12) | (rn << 5) | rt)


def encode_str_off(rt, rn, imm):
    imm12 = (imm // 8) & 0xFFF
    return encode_u32(0xF9000000 | (imm12 << 10) | (rn << 5) | rt)


def encode_ldr_off(rt, rn, imm):
    imm12 = (imm // 8) & 0xFFF
    return encode_u32(0xF9400000 | (imm12 << 10) | (rn << 5) | rt)


def encode_movz(rd, imm16, shift=0, is64=True):
    hw = shift // 16
    sf = 1 if is64 else 0
    return encode_u32((sf << 31) | 0x52800000 | (hw << 21) | ((imm16 & 0xFFFF) << 5) | rd)


def encode_movk(rd, imm16, shift=0, is64=True):
    hw = shift // 16
    sf = 1 if is64 else 0
    return encode_u32((sf << 31) | 0x72800000 | (hw << 21) | ((imm16 & 0xFFFF) << 5) | rd)


def encode_ldrb_unsigned(rt, rn, imm12=0):
    return encode_u32(0x39400000 | ((imm12 & 0xFFF) << 10) | (rn << 5) | rt)


def encode_strb_unsigned(rt, rn, imm12=0):
    return encode_u32(0x39000000 | ((imm12 & 0xFFF) << 10) | (rn << 5) | rt)


def encode_cbnz(rt, source_va, target_va, is64=False):
    offset = target_va - source_va
    if offset % 4 != 0:
        raise ValueError("cbnz target must be 4-byte aligned")
    imm19 = offset >> 2
    if not (-(1 << 18) <= imm19 < (1 << 18)):
        raise ValueError("cbnz out of range")
    sf = 1 if is64 else 0
    return encode_u32((sf << 31) | 0x35000000 | ((imm19 & 0x7FFFF) << 5) | rt)


def encode_add_imm(rd, rn, imm12, shift=0, is64=True):
    sf = 1 if is64 else 0
    sh = 1 if shift else 0
    return encode_u32((sf << 31) | 0x11000000 | (sh << 22) | ((imm12 & 0xFFF) << 10) | (rn << 5) | rd)


def encode_svc(imm16=0):
    return encode_u32(0xD4000001 | ((imm16 & 0xFFFF) << 5))


def encode_mov_reg(rd, rm, is64=True):
    sf = 1 if is64 else 0
    return encode_u32((sf << 31) | 0x2A0003E0 | (rm << 16) | rd)


def encode_nop():
    return encode_u32(0xD503201F)


def mov_abs_x(reg, value):
    chunks = [
        value & 0xFFFF,
        (value >> 16) & 0xFFFF,
        (value >> 32) & 0xFFFF,
        (value >> 48) & 0xFFFF,
    ]
    code = bytearray()
    code += encode_movz(reg, chunks[0], 0)
    for idx in range(1, 4):
        if chunks[idx]:
            code += encode_movk(reg, chunks[idx], idx * 16)
    return code


def compute_slot_from_entry(entry_vaddr):
    if entry_vaddr < ELF_SLOT_BASE:
        return 0
    return (entry_vaddr - ELF_SLOT_BASE) // ELF_SLOT_STRIDE


def scratch_addr_for_entry(entry_vaddr):
    slot = compute_slot_from_entry(entry_vaddr)
    return SCRATCH_BASE + slot * SCRATCH_STRIDE


def build_shim(intent_id, original_bytes, func_vaddr, shim_vaddr, resume_offset, scratch_addr):
    stub = bytearray()

    stub += encode_str_pre(10, 31, -8)
    stub += mov_abs_x(10, scratch_addr)
    stub += encode_stp_off(0, 1, 10, 0)
    stub += encode_stp_off(2, 3, 10, 16)
    stub += encode_stp_off(4, 5, 10, 32)
    stub += encode_stp_off(6, 7, 10, 48)
    stub += encode_stp_off(8, 9, 10, 64)
    stub += encode_ldr_post(9, 31, 8)
    stub += encode_str_off(9, 10, 80)

    stub += encode_movz(8, 0x88)
    stub += encode_movz(0, intent_id & 0xFFFF)
    stub += encode_add_imm(1, 31, 0)
    stub += encode_svc(0)

    stub += mov_abs_x(10, scratch_addr)
    stub += encode_ldp_off(0, 1, 10, 0)
    stub += encode_ldp_off(2, 3, 10, 16)
    stub += encode_ldp_off(4, 5, 10, 32)
    stub += encode_ldp_off(6, 7, 10, 48)
    stub += encode_ldp_off(8, 9, 10, 64)
    stub += encode_ldr_off(10, 10, 80)
    stub += original_bytes[:SAVE_LEN]
    stub += encode_b(shim_vaddr + len(stub), func_vaddr + resume_offset)
    return bytes(stub)


def build_injection_return_shim(shim_base_vaddr):
    stub = bytearray()
    stub += encode_stp_pre(0, 1, 31, -16)
    stub += encode_movz(8, 0x89)
    stub += encode_svc(0)
    loop_va = shim_base_vaddr + len(stub)
    stub += encode_b(loop_va, loop_va)
    return bytes(stub)


def infer_intent_class(func_name: str, app_name: str = "") -> str:
    app = app_name.lower()
    if "compute" in app:
        return "Compute"
    if "background" in app:
        return "Background"
    if os.path.basename(app).startswith("io") or "io_" in app or "/io_" in app or "io_workload" in app:
        return "IO"
    name = func_name.lower()
    if any(k in name for k in ["net", "send", "recv", "socket", "http", "tcp", "udp"]):
        return "IO"
    if any(k in name for k in ["crypto", "hash", "encrypt", "decrypt", "aes", "sha", "sign"]):
        return "Compute"
    if any(k in name for k in ["render", "draw", "ui", "display", "paint", "frame"]):
        return "RealTime"
    if any(k in name for k in ["log", "audit", "trace", "debug", "monitor"]):
        return "Background"
    if any(k in name for k in ["init", "start", "main", "setup", "boot"]):
        return "System"
    return "Unknown"


def resolve_app_layout(binary_path):
    app_name = os.path.basename(binary_path)
    return SHIM_LAYOUTS.get(app_name, (0x47F00000, 0x47FFF000, "Unknown"))


def should_skip_hook(binary_path, func_name):
    app_name = os.path.basename(binary_path)
    if app_name in NO_HOOK_APPS:
        return True
    if func_name == "_start":
        return True
    return func_name in SKIP_HOOK_SYMBOLS.get(app_name, set())


def infer_capabilities(func_name: str) -> list:
    name = func_name.lower()
    caps = []
    if any(k in name for k in ["net", "send", "recv", "socket"]):
        caps.extend(["NET_SEND", "NET_RECV"])
    if any(k in name for k in ["read", "open", "load", "fread"]):
        caps.append("FS_READ")
    if any(k in name for k in ["write", "save", "fwrite", "store"]):
        caps.append("FS_WRITE")
    if any(k in name for k in ["crypto", "encrypt", "decrypt", "hash"]):
        caps.append("CRYPTO_ACCEL")
    if any(k in name for k in ["mmap", "alloc", "malloc", "memory"]):
        caps.append("MEM_MAP")
    return caps or ["OBSERVE"]


def write_binary_mlm(manifest, default_cap_bitmap, path):
    entries = manifest["intent_registry"]
    with open(path, "wb") as handle:
        handle.write(b"MAYA")
        handle.write(struct.pack("<III", MLMB_VERSION, len(entries), 0))
        handle.write(struct.pack("<Q", default_cap_bitmap))
        handle.write(struct.pack("<Q", int(manifest["inject_return_vaddr"], 16)))
        handle.write(struct.pack("<Q", int(manifest["shim_load_addr"], 16)))
        handle.write(struct.pack("<Q", int(manifest["reentrancy_guard_addr"], 16)))
        handle.write(struct.pack("<Q", int(manifest["scratch_addr"], 16)))
        for entry in entries:
            intent_id = entry["intent_id"]
            entry_vaddr = int(entry["vaddr"], 16)
            name_hash = fnv1a_64(entry["name"])
            intent_class = {
                "Unknown": 0,
                "Compute": 1,
                "IO": 2,
                "RealTime": 3,
                "Background": 4,
                "System": 5,
            }.get(entry["intent_class"], 0)
            cap_rights = 0x0060
            name = entry["name"][:16].encode("utf-8").ljust(16, b"\x00")
            handle.write(struct.pack("<QQQHHi", intent_id, entry_vaddr, name_hash, intent_class, cap_rights, 0))
            handle.write(name)


def patch_binary(binary_path):
    with open(binary_path, "rb") as handle:
        original_data = bytearray(handle.read())
    with open(binary_path, "rb") as handle:
        elffile = ELFFile(handle)
        functions, exec_segments = discover_functions(elffile)
        entry_vaddr = int(elffile.header["e_entry"])

    if not functions:
        raise SystemExit("No functions discovered")

    shim_load_addr, guard_addr, app_intent_class = resolve_app_layout(binary_path)
    scratch_addr = scratch_addr_for_entry(entry_vaddr)
    shim_blob = bytearray()
    manifest = []
    patched = []
    skipped = []
    app_name = os.path.basename(binary_path)

    mexe_path = binary_path + ".mexe"
    mshm_path = binary_path + ".mshm"
    mlm_path = binary_path + ".mlm"
    mlmb_path = binary_path + ".mlmb"
    inject_return_vaddr = shim_load_addr

    if app_name in NO_HOOK_APPS:
        with open(mexe_path, "wb") as handle:
            handle.write(original_data)
        with open(mshm_path, "wb") as handle:
            handle.write(b"")
        manifest_blob = {
            "maya_app_id": app_name,
            "arch": "aarch64",
            "shim_load_addr": "0x{:x}".format(shim_load_addr),
            "reentrancy_guard_addr": "0x{:x}".format(guard_addr),
            "scratch_addr": "0x{:x}".format(scratch_addr),
            "inject_return_vaddr": "0x{:x}".format(inject_return_vaddr),
            "intent_registry": [],
        }
        with open(mlm_path, "w", encoding="utf-8") as handle:
            json.dump(manifest_blob, handle, indent=2)
        write_binary_mlm(manifest_blob, DEFAULT_CAP_BITMAP, mlmb_path)
        print("No hooks for {} (legacy C workload)".format(app_name))
        print("Output: {}".format(mexe_path))
        print("Shim blob: {}".format(mshm_path))
        print("Manifest: {}".format(mlm_path))
        print("Binary MLM: {}".format(mlmb_path))
        return

    for func in functions:
        if should_skip_hook(binary_path, func["name"]):
            skipped.append((func["name"], func["vaddr"], "hook disabled for entry symbol"))
            continue

        file_offset = file_offset_for_vaddr(exec_segments, func["vaddr"])
        if file_offset is None:
            skipped.append((func["name"], func["vaddr"], "no file offset"))
            continue

        analysis, reason = analyze_patch_window(original_data, file_offset, func["vaddr"])
        if analysis is None:
            skipped.append((func["name"], func["vaddr"], reason))
            continue

        intent_id = INTENT_BASE + len(patched)
        current_shim_vaddr = shim_load_addr + len(shim_blob)
        shim = build_shim(
            intent_id,
            analysis["saved_bytes"],
            func["vaddr"],
            current_shim_vaddr,
            analysis["save_len"],
            scratch_addr,
        )

        patch = encode_b(func["vaddr"], current_shim_vaddr)
        original_data[file_offset:file_offset + PATCH_LEN] = patch
        patched.append(
            {
                "name": func["name"],
                "vaddr": func["vaddr"],
                "save_len": analysis["save_len"],
                "before": analysis["saved_bytes"],
                "after": patch,
                "instructions": analysis["instructions"],
                "intent_id": intent_id,
            }
        )
        shim_blob += shim
        manifest.append(
            {
                "intent_id": intent_id,
                "name": func["name"],
                "vaddr": "0x{:x}".format(func["vaddr"]),
                "intent_class": infer_intent_class(func["name"], app_intent_class),
                "capability_requirements": infer_capabilities(func["name"]),
            }
        )

    if not patched:
        raise SystemExit("No functions were safely patchable")

    inject_return_offset = len(shim_blob)
    inject_return_vaddr = shim_load_addr + inject_return_offset
    inject_return_bytes = build_injection_return_shim(inject_return_vaddr)
    shim_blob += inject_return_bytes

    with open(mexe_path, "wb") as handle:
        handle.write(original_data)
    with open(mshm_path, "wb") as handle:
        handle.write(shim_blob)

    manifest_blob = {
        "maya_app_id": os.path.basename(binary_path),
        "arch": "aarch64",
        "shim_load_addr": "0x{:x}".format(shim_load_addr),
        "reentrancy_guard_addr": "0x{:x}".format(guard_addr),
        "scratch_addr": "0x{:x}".format(scratch_addr),
        "inject_return_vaddr": "0x{:x}".format(inject_return_vaddr),
        "intent_registry": manifest,
    }
    with open(mlm_path, "w", encoding="utf-8") as handle:
        json.dump(manifest_blob, handle, indent=2)
    write_binary_mlm(manifest_blob, DEFAULT_CAP_BITMAP, mlmb_path)

    print("Patched {} functions".format(len(patched)))
    for item in patched:
        print("  intent {:03d} -> {} @ 0x{:x}".format(item["intent_id"], item["name"], item["vaddr"]))
    if skipped:
        print("Skipped {} functions".format(len(skipped)))
        for name, vaddr, reason in skipped[:10]:
            print("  {} @ 0x{:x}: {}".format(name, vaddr, reason))
    print("Output: {}".format(mexe_path))
    print("Shim blob: {}".format(mshm_path))
    print("Manifest: {}".format(mlm_path))
    print("Binary MLM: {}".format(mlmb_path))


def main(argv):
    if len(argv) != 2:
        raise SystemExit("usage: mar_aarch64.py <aarch64-elf>")
    patch_binary(argv[1])


if __name__ == "__main__":
    main(sys.argv)

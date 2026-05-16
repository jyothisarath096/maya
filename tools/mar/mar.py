#!/usr/bin/env python3

import json
import os
import struct
import sys

from capstone import CS_ARCH_X86, CS_MODE_64, Cs
from elftools.elf.elffile import ELFFile


PT_LOAD = 1
PF_X = 1
PATCH_LEN = 5
INTENT_BASE = 100
SHIM_LOAD_ADDR = 0x7F000000
PROLOGUES = (
    b"\x55\x48\x89\xe5",
    b"\xf3\x0f\x1e\xfa",
)
MAX_ANALYZE_BYTES = 64


def file_offset_for_vaddr(segments, vaddr):
    for seg in segments:
        start = seg["vaddr"]
        end = start + seg["filesz"]
        if start <= vaddr < end:
            return seg["offset"] + (vaddr - start)
    return None


def discover_functions(elffile):
    functions = {}

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
            functions[vaddr] = {
                "name": name,
                "size": symbol["st_size"],
            }

    exec_segments = []
    for segment in elffile.iter_segments():
        if segment["p_type"] != "PT_LOAD":
            continue
        if (segment["p_flags"] & PF_X) == 0:
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
        for prologue in PROLOGUES:
            start = 0
            while True:
                pos = data.find(prologue, start)
                if pos == -1:
                    break
                vaddr = seg["vaddr"] + pos
                if vaddr not in functions:
                    functions[vaddr] = {
                        "name": "sub_{:x}".format(vaddr),
                        "size": 0,
                    }
                start = pos + 1

    ordered = []
    for vaddr in sorted(functions):
        ordered.append(
            {
                "vaddr": vaddr,
                "name": functions[vaddr]["name"],
                "size": functions[vaddr]["size"],
            }
        )

    return ordered, exec_segments


def is_unsupported_relative_control(insn_bytes):
    if not insn_bytes:
        return False
    op0 = insn_bytes[0]
    if op0 in (0xE8, 0xE9, 0xEB):
        return True
    return len(insn_bytes) >= 2 and op0 == 0x0F and 0x80 <= insn_bytes[1] <= 0x8F


def analyze_patch_window(binary, file_offset, func_vaddr):
    code = bytes(binary[file_offset:file_offset + MAX_ANALYZE_BYTES])
    md = Cs(CS_ARCH_X86, CS_MODE_64)
    saved = bytearray()
    instructions = []

    for insn in md.disasm(code, func_vaddr):
        insn_bytes = bytes(insn.bytes)
        if is_unsupported_relative_control(insn_bytes):
            return None, "relative control-flow opcode at entry ({})".format(insn.mnemonic)
        instructions.append(
            {
                "address": insn.address,
                "size": insn.size,
                "mnemonic": insn.mnemonic,
                "op_str": insn.op_str,
            }
        )
        saved += insn_bytes
        if len(saved) >= PATCH_LEN:
            return {
                "save_len": len(saved),
                "saved_bytes": bytes(saved),
                "instructions": instructions,
            }, None

    return None, "could not decode 5 bytes of complete instructions"


def build_shim(intent_id, original_bytes, func_vaddr, shim_vaddr, resume_offset):
    stub = bytearray()

    stub += b"\x50"  # push rax
    stub += b"\x51"  # push rcx
    stub += b"\x52"  # push rdx
    stub += b"\x56"  # push rsi
    stub += b"\x57"  # push rdi
    stub += b"\x41\x50"  # push r8
    stub += b"\x41\x51"  # push r9
    stub += b"\x41\x52"  # push r10
    stub += b"\x41\x53"  # push r11

    stub += b"\x48\xb8" + struct.pack("<Q", 0x7FFFFF00)  # mov rax, 0x7FFFFF00
    stub += b"\x80\x38\x00"  # cmp byte ptr [rax], 0
    jne_offset = len(stub)
    stub += b"\x75\x00"  # jne .skip_telemetry
    stub += b"\xc6\x00\x01"  # mov byte ptr [rax], 1

    stub += b"\x48\xc7\xc0\x88\x00\x00\x00"  # mov rax, 0x88
    stub += b"\x48\xc7\xc7" + struct.pack("<I", intent_id)  # mov rdi, imm32
    stub += b"\x48\x89\xe6"  # mov rsi, rsp
    stub += b"\x0f\x05"  # syscall
    stub += b"\x48\xb8" + struct.pack("<Q", 0x7FFFFF00)  # mov rax, 0x7FFFFF00
    stub += b"\xc6\x00\x00"  # mov byte ptr [rax], 0

    skip_target = len(stub)
    stub[jne_offset + 1] = skip_target - (jne_offset + 2)

    stub += b"\x41\x5b"  # pop r11
    stub += b"\x41\x5a"  # pop r10
    stub += b"\x41\x59"  # pop r9
    stub += b"\x41\x58"  # pop r8
    stub += b"\x5f"  # pop rdi
    stub += b"\x5e"  # pop rsi
    stub += b"\x5a"  # pop rdx
    stub += b"\x59"  # pop rcx
    stub += b"\x58"  # pop rax

    stub += original_bytes

    return_target = func_vaddr + resume_offset
    jmp_from = shim_vaddr + len(stub) + PATCH_LEN
    rel32 = return_target - jmp_from
    stub += b"\xE9" + struct.pack("<i", rel32)
    return bytes(stub)


def patch_binary(binary_path):
    with open(binary_path, "rb") as handle:
        original_data = bytearray(handle.read())

    with open(binary_path, "rb") as handle:
        elffile = ELFFile(handle)
        functions, exec_segments = discover_functions(elffile)

    if not functions:
        raise SystemExit("No functions discovered")

    shim_blob = bytearray()
    manifest = []
    patched = []
    skipped = []

    for index, func in enumerate(functions):
        file_offset = file_offset_for_vaddr(exec_segments, func["vaddr"])
        if file_offset is None:
            skipped.append((func["name"], func["vaddr"], "no file offset"))
            continue

        analysis, reason = analyze_patch_window(original_data, file_offset, func["vaddr"])
        if analysis is None:
            skipped.append((func["name"], func["vaddr"], reason))
            continue

        original = analysis["saved_bytes"]
        intent_id = INTENT_BASE + len(patched)
        current_shim_vaddr = SHIM_LOAD_ADDR + len(shim_blob)
        shim = build_shim(
            intent_id,
            original,
            func["vaddr"],
            current_shim_vaddr,
            analysis["save_len"],
        )

        rel32 = current_shim_vaddr - (func["vaddr"] + PATCH_LEN)
        patch = b"\xE9" + struct.pack("<i", rel32)
        original_data[file_offset:file_offset + PATCH_LEN] = patch

        patched.append(
            {
                "name": func["name"],
                "vaddr": func["vaddr"],
                "save_len": analysis["save_len"],
                "before": original[:analysis["save_len"]],
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
            }
        )

    if not patched:
        raise SystemExit("No functions were safely patchable")

    mexe_path = binary_path + ".mexe"
    mshm_path = binary_path + ".mshm"
    mlm_path = binary_path + ".mlm"

    with open(mexe_path, "wb") as handle:
        handle.write(original_data)

    with open(mshm_path, "wb") as handle:
        handle.write(shim_blob)

    with open(mlm_path, "w", encoding="utf-8") as handle:
        json.dump(
            {
                "maya_app_id": os.path.basename(binary_path),
                "shim_load_addr": "0x{:x}".format(SHIM_LOAD_ADDR),
                "intent_registry": manifest,
            },
            handle,
            indent=2,
        )
        handle.write("\n")

    print("Input: {}".format(binary_path))
    print("Output: {}".format(mexe_path))
    print("Shim blob: {}".format(mshm_path))
    print("Manifest: {}".format(mlm_path))
    print("Functions discovered: {}".format(len(functions)))
    print("Functions patched safely: {}".format(len(patched)))
    print("Functions skipped: {}".format(len(skipped)))
    print("Shim load addr: 0x{:x}".format(SHIM_LOAD_ADDR))
    print("Shim size: {}".format(len(shim_blob)))

    print("Safe to shim:")
    for item in patched:
        print(
            "  {name} @ 0x{addr:x} save_len={save_len} before={before} after={after}".format(
                name=item["name"],
                addr=item["vaddr"],
                save_len=item["save_len"],
                before=item["before"].hex(),
                after=item["after"].hex(),
            )
        )

    print("Skipped:")
    for name, addr, reason in skipped:
        print("  {} @ 0x{:x} skipped: {}".format(name, addr, reason))


def main():
    if len(sys.argv) != 2:
        print("Usage: python3 mar.py <binary_path>")
        raise SystemExit(1)

    patch_binary(sys.argv[1])


if __name__ == "__main__":
    main()

#![allow(dead_code)]

extern crate alloc;

use alloc::vec;

use crate::KernelError;

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const EM_AARCH64: u16 = 183;
const PT_LOAD: u32 = 1;
const PF_X: u32 = 1 << 0;
const PF_W: u32 = 1 << 1;
const MAX_SEGMENTS_PER_PROC: usize = 8;
const ELF_SLOT_BASE: u64 = 0x4180_0000;
const ELF_SLOT_STRIDE: u64 = 0x0010_0000;
const SHIM_LOAD_ADDR_BASE: u64 = 0x47F0_0000;
const SHIM_LOAD_ADDR_STRIDE: u64 = 0x0001_0000;
const SCRATCH_BASE: u64 = 0x47FE_0000;
const SCRATCH_STRIDE: u64 = 0x1000;
const REENTRANCY_GUARD_ADDR_BASE: u64 = 0x47FF_F000;
const STACK_BASE: u64 = 0x5000_0000;
const STACK_STRIDE: u64 = 0x0001_0000;
const STACK_SIZE: usize = 65536;

#[repr(C)]
struct Elf64ProgramHeader {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: u64,
    p_paddr: u64,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

pub struct LoadedElf {
    pub entry: u64,
    pub stack_top: u64,
    pub scratch_addr: u64,
    pub ttbr0: u64,
    pub asid: u16,
    pub segments: [(u64, usize); MAX_SEGMENTS_PER_PROC],
    pub seg_count: usize,
}

pub fn slot_for_entry(entry: u64) -> usize {
    if entry < ELF_SLOT_BASE {
        0
    } else {
        ((entry - ELF_SLOT_BASE) / ELF_SLOT_STRIDE) as usize
    }
}

fn stack_base_for_slot(slot: usize) -> u64 {
    STACK_BASE + (slot as u64 * STACK_STRIDE)
}

fn shim_base_for_slot(slot: usize) -> u64 {
    SHIM_LOAD_ADDR_BASE + (slot as u64 * SHIM_LOAD_ADDR_STRIDE)
}

fn scratch_base_for_slot(slot: usize) -> u64 {
    SCRATCH_BASE + (slot as u64 * SCRATCH_STRIDE)
}

fn guard_base_for_slot(slot: usize) -> u64 {
    REENTRANCY_GUARD_ADDR_BASE + (slot as u64 * 0x1000)
}

pub fn load(elf_data: &[u8], asid: u16) -> Result<LoadedElf, KernelError> {
    if elf_data.len() < 64 || elf_data[0..4] != ELF_MAGIC {
        return Err(KernelError::InvalidElf);
    }
    if elf_data[4] != ELFCLASS64 || elf_data[5] != ELFDATA2LSB {
        return Err(KernelError::InvalidElf);
    }

    let e_type = u16::from_le_bytes([elf_data[16], elf_data[17]]);
    let e_machine = u16::from_le_bytes([elf_data[18], elf_data[19]]);
    let e_entry = u64::from_le_bytes(elf_data[24..32].try_into().unwrap());
    let e_phoff = u64::from_le_bytes(elf_data[32..40].try_into().unwrap());
    let e_phentsize = u16::from_le_bytes([elf_data[54], elf_data[55]]);
    let e_phnum = u16::from_le_bytes([elf_data[56], elf_data[57]]);

    if (e_type != ET_EXEC && e_type != ET_DYN) || e_machine != EM_AARCH64 {
        return Err(KernelError::InvalidElf);
    }
    if e_phentsize as usize != core::mem::size_of::<Elf64ProgramHeader>() {
        return Err(KernelError::InvalidElf);
    }

    let load_base = if e_type == ET_DYN { 0x0040_0000 } else { 0 };
    let ttbr0 = crate::memory::vmm::alloc_user_table().ok_or(KernelError::ElfLoadFailed)?;
    let mut segments = [(0u64, 0usize); MAX_SEGMENTS_PER_PROC];
    let mut seg_count = 0usize;

    struct SegInfo {
        vaddr: u64,
        memsz: u64,
        filesz: u64,
        offset: u64,
        flags: u32,
    }

    let mut seg_infos: alloc::vec::Vec<SegInfo> = alloc::vec::Vec::new();

    for i in 0..e_phnum as usize {
        let ph_offset = e_phoff as usize + i * e_phentsize as usize;
        if ph_offset + core::mem::size_of::<Elf64ProgramHeader>() > elf_data.len() {
            return Err(KernelError::InvalidElf);
        }
        let p_type = u32::from_le_bytes(elf_data[ph_offset..ph_offset + 4].try_into().unwrap());
        if p_type != PT_LOAD {
            continue;
        }
        let p_offset = u64::from_le_bytes(elf_data[ph_offset + 8..ph_offset + 16].try_into().unwrap());
        let p_flags = u32::from_le_bytes(elf_data[ph_offset + 4..ph_offset + 8].try_into().unwrap());
        let p_vaddr = u64::from_le_bytes(elf_data[ph_offset + 16..ph_offset + 24].try_into().unwrap());
        let p_filesz = u64::from_le_bytes(elf_data[ph_offset + 32..ph_offset + 40].try_into().unwrap());
        let p_memsz = u64::from_le_bytes(elf_data[ph_offset + 40..ph_offset + 48].try_into().unwrap());
        if p_offset as usize + p_filesz as usize > elf_data.len() || p_memsz < p_filesz {
            return Err(KernelError::InvalidElf);
        }
        seg_infos.push(SegInfo {
            vaddr: load_base + p_vaddr,
            memsz: p_memsz,
            filesz: p_filesz,
            offset: p_offset,
            flags: p_flags,
        });
    }

    const PAGE_SIZE: u64 = 4096;
    for seg in seg_infos.iter() {
        let data_end = seg.offset as usize + seg.filesz as usize;
        let data = &elf_data[seg.offset as usize..data_end];
        let mut segment_data = vec![0u8; seg.memsz as usize];
        segment_data[..seg.filesz as usize].copy_from_slice(data);

        let seg_page_start = seg.vaddr & !(PAGE_SIZE - 1);
        let seg_page_end = (seg.vaddr + seg.memsz + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let needs_exec = seg_infos.iter().any(|other| {
            let other_page_start = other.vaddr & !(PAGE_SIZE - 1);
            let other_page_end = (other.vaddr + other.memsz + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
            other.flags & PF_X != 0
                && other_page_start < seg_page_end
                && other_page_end > seg_page_start
        });

        let exec = (seg.flags & PF_X != 0) || needs_exec;
        let writable = seg.flags & PF_W != 0;
        crate::memory::vmm::map_user_segment(
            ttbr0,
            seg.vaddr,
            &segment_data,
            writable,
            exec,
        )?;
        if seg_count < MAX_SEGMENTS_PER_PROC {
            segments[seg_count] = (seg.vaddr, seg.memsz as usize);
            seg_count += 1;
        }
    }
    let slot = slot_for_entry(load_base + e_entry);
    let stack_base = stack_base_for_slot(slot);
    let scratch_addr = scratch_base_for_slot(slot);
    let stack = vec![0u8; STACK_SIZE];
    crate::memory::vmm::map_user_segment(ttbr0, stack_base, &stack, true, false)?;
    let stack_top = stack_base + STACK_SIZE as u64;

    Ok(LoadedElf {
        entry: load_base + e_entry,
        stack_top,
        scratch_addr,
        ttbr0,
        asid,
        segments,
        seg_count,
    })
}

pub fn map_agentic_runtime(
    ttbr0: u64,
    shim_data: &[u8],
    shim_load_addr: u64,
    scratch_addr: u64,
    guard_addr: u64,
) -> Result<(), KernelError> {
    crate::memory::vmm::map_user_segment(ttbr0, shim_load_addr, shim_data, false, true)?;
    let scratch = vec![0u8; 128];
    crate::memory::vmm::map_user_segment(ttbr0, scratch_addr, &scratch, true, false)?;
    let guard = vec![0u8; 4096];
    crate::memory::vmm::map_user_segment(ttbr0, guard_addr, &guard, true, false)?;
    Ok(())
}

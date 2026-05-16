#![allow(dead_code)]

use alloc::vec::Vec;

use crate::{KernelError, proc::memory};

const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;
const EM_X86_64: u16 = 62;
const PT_LOAD: u32 = 1;

#[repr(C)]
struct Elf64Header {
    e_ident: [u8; 16],
    e_type: u16,
    e_machine: u16,
    e_version: u32,
    e_entry: u64,
    e_phoff: u64,
    e_shoff: u64,
    e_flags: u32,
    e_ehsize: u16,
    e_phentsize: u16,
    e_phnum: u16,
    e_shentsize: u16,
    e_shnum: u16,
    e_shstrndx: u16,
}

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
    pub cr3: u64,
    pub entry: u64,
    pub stack_top: u64,
    pub segments: Vec<(u64, usize)>,
}

pub fn load(elf_data: &[u8]) -> Result<LoadedElf, KernelError> {
    if elf_data.len() < core::mem::size_of::<Elf64Header>() {
        return Err(KernelError::InvalidElf);
    }
    if elf_data[0..4] != ELF_MAGIC {
        return Err(KernelError::InvalidElf);
    }
    if elf_data[4] != ELFCLASS64 || elf_data[5] != ELFDATA2LSB {
        return Err(KernelError::InvalidElf);
    }

    // Safe field extraction using byte reads (no alignment requirement)
    let e_type = u16::from_le_bytes([elf_data[16], elf_data[17]]);
    let e_machine = u16::from_le_bytes([elf_data[18], elf_data[19]]);
    let e_entry = u64::from_le_bytes([
        elf_data[24], elf_data[25], elf_data[26], elf_data[27],
        elf_data[28], elf_data[29], elf_data[30], elf_data[31],
    ]);
    let e_phoff = u64::from_le_bytes([
        elf_data[32], elf_data[33], elf_data[34], elf_data[35],
        elf_data[36], elf_data[37], elf_data[38], elf_data[39],
    ]);
    let e_phentsize = u16::from_le_bytes([elf_data[54], elf_data[55]]);
    let e_phnum = u16::from_le_bytes([elf_data[56], elf_data[57]]);

    if (e_type != ET_EXEC && e_type != ET_DYN) || e_machine != EM_X86_64 {
        return Err(KernelError::InvalidElf);
    }

    let load_base: u64 = if e_type == ET_DYN { 0x0040_0000 } else { 0 };

    let ph_end = e_phoff as usize
        + e_phnum as usize * e_phentsize as usize;
    if ph_end > elf_data.len()
        || e_phentsize as usize
            != core::mem::size_of::<Elf64ProgramHeader>()
    {
        return Err(KernelError::InvalidElf);
    }

    let cr3 = memory::create_address_space()?;
    let mut segments = Vec::new();

    for i in 0..e_phnum as usize {
        let ph_offset = e_phoff as usize + i * e_phentsize as usize;
        // Safe byte-by-byte program header reading
        let p_type = u32::from_le_bytes([
            elf_data[ph_offset], elf_data[ph_offset+1],
            elf_data[ph_offset+2], elf_data[ph_offset+3],
        ]);
        if p_type != PT_LOAD { continue; }

        let p_flags = u32::from_le_bytes([
            elf_data[ph_offset+4], elf_data[ph_offset+5],
            elf_data[ph_offset+6], elf_data[ph_offset+7],
        ]);
        let p_offset = u64::from_le_bytes([
            elf_data[ph_offset+8],  elf_data[ph_offset+9],
            elf_data[ph_offset+10], elf_data[ph_offset+11],
            elf_data[ph_offset+12], elf_data[ph_offset+13],
            elf_data[ph_offset+14], elf_data[ph_offset+15],
        ]);
        let p_vaddr = u64::from_le_bytes([
            elf_data[ph_offset+16], elf_data[ph_offset+17],
            elf_data[ph_offset+18], elf_data[ph_offset+19],
            elf_data[ph_offset+20], elf_data[ph_offset+21],
            elf_data[ph_offset+22], elf_data[ph_offset+23],
        ]);
        let p_filesz = u64::from_le_bytes([
            elf_data[ph_offset+32], elf_data[ph_offset+33],
            elf_data[ph_offset+34], elf_data[ph_offset+35],
            elf_data[ph_offset+36], elf_data[ph_offset+37],
            elf_data[ph_offset+38], elf_data[ph_offset+39],
        ]);
        let p_memsz = u64::from_le_bytes([
            elf_data[ph_offset+40], elf_data[ph_offset+41],
            elf_data[ph_offset+42], elf_data[ph_offset+43],
            elf_data[ph_offset+44], elf_data[ph_offset+45],
            elf_data[ph_offset+46], elf_data[ph_offset+47],
        ]);

        let data_start = p_offset as usize;
        let data_end = data_start.saturating_add(p_filesz as usize);
        if data_end > elf_data.len() || p_memsz < p_filesz {
            return Err(KernelError::InvalidElf);
        }

        let segment_data = &elf_data[data_start..data_end];
        let writable = p_flags & 2 != 0;
        let executable = p_flags & 1 != 0;

        memory::map_segment(cr3, load_base + p_vaddr, segment_data,
            writable, executable)?;
        segments.push((load_base + p_vaddr, p_filesz as usize));

        if p_memsz > p_filesz {
            let bss_len = (p_memsz - p_filesz) as usize;
            let zeros = alloc::vec![0u8; bss_len];
            memory::map_segment(
                cr3,
                load_base + p_vaddr + p_filesz,
                &zeros,
                writable,
                executable,
            )?;
            segments.push((load_base + p_vaddr + p_filesz, bss_len));
        }
    }

    let stack_top = 0x0000_7FFF_FFFF_0000u64;
    let stack_size = 64 * 4096;
    memory::alloc_stack(cr3, stack_top, stack_size)?;
    segments.push((stack_top - stack_size as u64, stack_size));

    Ok(LoadedElf {
        cr3,
        entry: load_base + e_entry,
        stack_top,
        segments,
    })
}

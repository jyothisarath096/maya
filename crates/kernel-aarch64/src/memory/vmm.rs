#![allow(dead_code)]

use crate::{memory::pmm, KernelError};

const PAGE_SIZE: usize = 4096;
const PTE_VALID: u64 = 1 << 0;
const PTE_TABLE: u64 = 1 << 1;
const PTE_PAGE: u64 = 1 << 1;
const PTE_AF: u64 = 1 << 10;
const PTE_SH_IS: u64 = 3 << 8;
const PTE_AP_RW_USER: u64 = 1 << 6;
const PTE_AP_RO_USER: u64 = 3 << 6;
const PTE_UXN: u64 = 1 << 54;
const PTE_PXN: u64 = 1 << 53;
const PTE_ATTRINDX_NORMAL: u64 = 1 << 2;
const ADDR_MASK: u64 = 0x0000_FFFF_FFFF_F000;

pub fn init() {}

pub fn alloc_user_table() -> Option<u64> {
    let frame = pmm::alloc_frame()?;
    unsafe {
        core::ptr::write_bytes(pmm::phys_to_virt(frame) as *mut u8, 0, PAGE_SIZE);
    }
    Some(frame)
}

pub fn map_user_segment(
    ttbr0: u64,
    vaddr: u64,
    data: &[u8],
    writable: bool,
    executable: bool,
) -> Result<(), KernelError> {
    if data.is_empty() {
        return Ok(());
    }

    let start_vaddr = vaddr & !(PAGE_SIZE as u64 - 1);
    let end_vaddr = (vaddr + data.len() as u64 + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1);
    let mut cur_vaddr = start_vaddr;

    while cur_vaddr < end_vaddr {
        let frame_phys = pmm::alloc_frame().ok_or(KernelError::ElfLoadFailed)?;
        let page_start = cur_vaddr;
        let page_end = cur_vaddr + PAGE_SIZE as u64;
        let copy_start = page_start.max(vaddr);
        let copy_end = page_end.min(vaddr + data.len() as u64);
        let copy_len = copy_end.saturating_sub(copy_start) as usize;
        let src_offset = copy_start.saturating_sub(vaddr) as usize;
        let dst_offset = copy_start.saturating_sub(page_start) as usize;

        unsafe {
            let dst = pmm::phys_to_virt(frame_phys) as *mut u8;
            core::ptr::write_bytes(dst, 0, PAGE_SIZE);
            if copy_len != 0 {
                core::ptr::copy_nonoverlapping(
                    data[src_offset..src_offset + copy_len].as_ptr(),
                    dst.add(dst_offset),
                    copy_len,
                );
            }

            let pte_ptr = get_or_create_pte(ttbr0, cur_vaddr).ok_or(KernelError::ElfLoadFailed)?;
            let mut pte = frame_phys
                | PTE_VALID
                | PTE_PAGE
                | PTE_AF
                | PTE_SH_IS
                | PTE_ATTRINDX_NORMAL;
            pte |= if writable { PTE_AP_RW_USER } else { PTE_AP_RO_USER };
            if !executable {
                pte |= PTE_UXN;
            }
            pte |= PTE_PXN;
            core::ptr::write_volatile(pte_ptr, pte);
        }

        cur_vaddr += PAGE_SIZE as u64;
    }

    unsafe {
        core::arch::asm!(
            "dsb ishst",
            "isb",
            options(nomem, nostack, preserves_flags)
        );
    }
    Ok(())
}

pub fn map_user_frames(
    ttbr0: u64,
    vaddr: u64,
    frames: &[u64],
    writable: bool,
    executable: bool,
) -> Result<(), KernelError> {
    if frames.is_empty() {
        return Ok(());
    }

    let start_vaddr = vaddr & !(PAGE_SIZE as u64 - 1);
    for (index, frame_phys) in frames.iter().copied().enumerate() {
        let cur_vaddr = start_vaddr + index as u64 * PAGE_SIZE as u64;
        unsafe {
            let pte_ptr = get_or_create_pte(ttbr0, cur_vaddr).ok_or(KernelError::VmmMapFailed)?;
            let mut pte = frame_phys
                | PTE_VALID
                | PTE_PAGE
                | PTE_AF
                | PTE_SH_IS
                | PTE_ATTRINDX_NORMAL;
            pte |= if writable { PTE_AP_RW_USER } else { PTE_AP_RO_USER };
            if !executable {
                pte |= PTE_UXN;
            }
            pte |= PTE_PXN;
            core::ptr::write_volatile(pte_ptr, pte);
        }
    }

    unsafe {
        core::arch::asm!(
            "dsb ishst",
            "isb",
            options(nomem, nostack, preserves_flags)
        );
    }
    Ok(())
}

pub fn unmap_user_range(ttbr0: u64, vaddr: u64, len: usize) {
    if len == 0 {
        return;
    }

    let start_vaddr = vaddr & !(PAGE_SIZE as u64 - 1);
    let end_vaddr = (vaddr + len as u64 + PAGE_SIZE as u64 - 1) & !(PAGE_SIZE as u64 - 1);
    let mut cur_vaddr = start_vaddr;

    while cur_vaddr < end_vaddr {
        unsafe {
            if let Some(pte_ptr) = get_existing_pte(ttbr0, cur_vaddr) {
                core::ptr::write_volatile(pte_ptr, 0);
            }
        }
        cur_vaddr += PAGE_SIZE as u64;
    }

    unsafe {
        core::arch::asm!(
            "dsb ishst",
            "tlbi vmalle1",
            "dsb ish",
            "isb",
            options(nomem, nostack, preserves_flags)
        );
    }
}

pub fn free_user_table(ttbr0: u64) {
    unsafe {
        free_table_level(ttbr0, 0);
    }
}

pub fn set_user_table(ttbr0: u64, asid: u16) {
    let ttbr0_val = ((asid as u64) << 48) | ttbr0;
    unsafe {
        core::arch::asm!(
            "msr ttbr0_el1, {val}",
            "isb",
            "tlbi vmalle1is",
            "dsb ish",
            "isb",
            val = in(reg) ttbr0_val,
            options(nomem, nostack)
        );
    }
}

pub fn kernel_ttbr1() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!(
            "mrs {value}, ttbr1_el1",
            value = out(reg) value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value & ADDR_MASK
}

unsafe fn get_or_create_pte(l0_phys: u64, vaddr: u64) -> Option<*mut u64> {
    let l0_idx = ((vaddr >> 39) & 0x1FF) as usize;
    let l1_idx = ((vaddr >> 30) & 0x1FF) as usize;
    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    let l3_idx = ((vaddr >> 12) & 0x1FF) as usize;

    let l0 = pmm::phys_to_virt(l0_phys) as *mut u64;
    let l0e = l0.add(l0_idx);
    if core::ptr::read_volatile(l0e) == 0 {
        let l1_phys = pmm::alloc_frame()?;
        core::ptr::write_bytes(pmm::phys_to_virt(l1_phys) as *mut u8, 0, PAGE_SIZE);
        core::ptr::write_volatile(l0e, l1_phys | PTE_VALID | PTE_TABLE);
    }
    let l1_phys = core::ptr::read_volatile(l0e) & ADDR_MASK;

    let l1 = pmm::phys_to_virt(l1_phys) as *mut u64;
    let l1e = l1.add(l1_idx);
    if core::ptr::read_volatile(l1e) == 0 {
        let l2_phys = pmm::alloc_frame()?;
        core::ptr::write_bytes(pmm::phys_to_virt(l2_phys) as *mut u8, 0, PAGE_SIZE);
        core::ptr::write_volatile(l1e, l2_phys | PTE_VALID | PTE_TABLE);
    }
    let l2_phys = core::ptr::read_volatile(l1e) & ADDR_MASK;

    let l2 = pmm::phys_to_virt(l2_phys) as *mut u64;
    let l2e = l2.add(l2_idx);
    if core::ptr::read_volatile(l2e) == 0 {
        let l3_phys = pmm::alloc_frame()?;
        core::ptr::write_bytes(pmm::phys_to_virt(l3_phys) as *mut u8, 0, PAGE_SIZE);
        core::ptr::write_volatile(l2e, l3_phys | PTE_VALID | PTE_TABLE);
    }
    let l3_phys = core::ptr::read_volatile(l2e) & ADDR_MASK;

    let l3 = pmm::phys_to_virt(l3_phys) as *mut u64;
    Some(l3.add(l3_idx))
}

unsafe fn get_existing_pte(l0_phys: u64, vaddr: u64) -> Option<*mut u64> {
    let l0_idx = ((vaddr >> 39) & 0x1FF) as usize;
    let l1_idx = ((vaddr >> 30) & 0x1FF) as usize;
    let l2_idx = ((vaddr >> 21) & 0x1FF) as usize;
    let l3_idx = ((vaddr >> 12) & 0x1FF) as usize;

    let l0 = pmm::phys_to_virt(l0_phys) as *mut u64;
    let l0e = core::ptr::read_volatile(l0.add(l0_idx));
    if l0e & PTE_VALID == 0 || l0e & PTE_TABLE == 0 {
        return None;
    }
    let l1 = pmm::phys_to_virt(l0e & ADDR_MASK) as *mut u64;
    let l1e = core::ptr::read_volatile(l1.add(l1_idx));
    if l1e & PTE_VALID == 0 || l1e & PTE_TABLE == 0 {
        return None;
    }
    let l2 = pmm::phys_to_virt(l1e & ADDR_MASK) as *mut u64;
    let l2e = core::ptr::read_volatile(l2.add(l2_idx));
    if l2e & PTE_VALID == 0 || l2e & PTE_TABLE == 0 {
        return None;
    }
    let l3 = pmm::phys_to_virt(l2e & ADDR_MASK) as *mut u64;
    Some(l3.add(l3_idx))
}

unsafe fn free_table_level(table_phys: u64, level: usize) {
    let table = pmm::phys_to_virt(table_phys) as *mut u64;
    for index in 0..512usize {
        let entry = core::ptr::read_volatile(table.add(index));
        if entry & PTE_VALID == 0 {
            continue;
        }
        let child_phys = entry & ADDR_MASK;
        if level == 3 || (entry & PTE_TABLE == 0) {
            pmm::free_frame(child_phys);
        } else {
            free_table_level(child_phys, level + 1);
        }
    }
    pmm::free_frame(table_phys);
}

#![allow(dead_code)]

use core::{cmp, ptr};

use x86_64::{
    VirtAddr,
    registers::control::Cr3,
    structures::paging::{Page, PageSize, PageTableFlags, Size4KiB},
};

use crate::{
    KernelError,
    memory::{pmm, vmm},
};

pub struct ProcessMemory {
    pub cr3: u64,
    pub base: u64,
    pub size: usize,
    pub stack_base: u64,
    pub stack_size: usize,
}

pub fn create_address_space() -> Result<u64, KernelError> {
    Ok(Cr3::read().0.start_address().as_u64())
}

pub fn map_segment(
    _cr3: u64,
    virt: u64,
    data: &[u8],
    writable: bool,
    executable: bool,
) -> Result<(), KernelError> {
    if data.is_empty() {
        return Ok(());
    }

    let start = VirtAddr::new(virt);
    let start_page = Page::<Size4KiB>::containing_address(start);
    let end_addr = start + (data.len() as u64 - 1);
    let end_page = Page::<Size4KiB>::containing_address(end_addr);
    let mut offset = 0usize;

    for page in Page::range_inclusive(start_page, end_page) {
        let frame = pmm::alloc_frame()?;
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        if writable {
            flags |= PageTableFlags::WRITABLE;
        }
        if !executable {
            flags |= PageTableFlags::NO_EXECUTE;
        }
        match vmm::map_page(page, frame, flags) {
            Ok(_) => {}
            Err(_) => {
                if vmm::is_page_mapped(page)? {
                    vmm::unmap_page(page).ok();
                    vmm::map_page(page, frame, flags)?;
                } else {
                    return Err(KernelError::VmmMapFailed);
                }
            }
        }

        let page_virt = page.start_address().as_u64();
        let copy_start = if offset == 0 {
            (virt - page_virt) as usize
        } else {
            0
        };
        let copy_len = cmp::min(Size4KiB::SIZE as usize - copy_start, data.len() - offset);
        let frame_virt = match vmm::phys_to_virt_addr(frame.start_address().as_u64()) {
            Ok(addr) => addr,
            Err(_) => return Err(KernelError::ProcessError),
        };

        unsafe {
            ptr::write_bytes(frame_virt.as_mut_ptr::<u8>(), 0, Size4KiB::SIZE as usize);
            ptr::copy_nonoverlapping(
                data[offset..offset + copy_len].as_ptr(),
                frame_virt.as_mut_ptr::<u8>().add(copy_start),
                copy_len,
            );
        }

        offset += copy_len;
    }

    Ok(())
}

pub fn alloc_stack(_cr3: u64, stack_top: u64, size: usize) -> Result<(), KernelError> {
    if size == 0 {
        return Ok(());
    }

    let stack_base = stack_top.saturating_sub(size as u64);
    let start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(stack_base));
    let end_page =
        Page::<Size4KiB>::containing_address(VirtAddr::new(stack_top.saturating_sub(1)));

    for page in Page::range_inclusive(start_page, end_page) {
        let frame = pmm::alloc_frame()?;
        vmm::map_page(
            page,
            frame,
            PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::NO_EXECUTE,
        )?;
        let frame_virt = match vmm::phys_to_virt_addr(frame.start_address().as_u64()) {
            Ok(addr) => addr,
            Err(_) => return Err(KernelError::ProcessError),
        };
        unsafe {
            ptr::write_bytes(frame_virt.as_mut_ptr::<u8>(), 0, Size4KiB::SIZE as usize);
        }
    }

    Ok(())
}

pub fn unmap_process_pages(segments: &[(u64, usize)]) {
    for &(vaddr, size) in segments {
        if size == 0 {
            continue;
        }

        let start = Page::<Size4KiB>::containing_address(VirtAddr::new(vaddr));
        let end_addr = vaddr + size as u64 - 1;
        let end = Page::<Size4KiB>::containing_address(VirtAddr::new(end_addr));

        for page in Page::range_inclusive(start, end) {
            crate::memory::vmm::unmap_page(page).ok();
        }
    }
}

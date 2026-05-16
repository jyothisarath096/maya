#![allow(dead_code)]

use linked_list_allocator::LockedHeap;
use x86_64::{
    VirtAddr,
    structures::paging::{Page, PageTableFlags, Size4KiB},
};

use crate::KernelError;

use super::{pmm, vmm};

const HEAP_START: u64 = 0xFFFF_8800_0000_0000;
const HEAP_SIZE: u64 = 1024 * 1024;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn init(_phys_mem_offset: VirtAddr) -> Result<(), KernelError> {
    let heap_start = VirtAddr::new(HEAP_START);
    let heap_end = heap_start + HEAP_SIZE;

    let mut virt = heap_start;
    while virt < heap_end {
        let page = Page::<Size4KiB>::containing_address(virt);
        let frame = pmm::alloc_frame().map_err(|_| KernelError::HeapInitFailed)?;
        vmm::map_page(
            page,
            frame,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
        )
        .map_err(|_| KernelError::HeapInitFailed)?;
        virt += 4096u64;
    }

    unsafe {
        ALLOCATOR
            .lock()
            .init(heap_start.as_mut_ptr(), HEAP_SIZE as usize);
    }

    Ok(())
}

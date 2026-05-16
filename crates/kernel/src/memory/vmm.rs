#![allow(dead_code)]

use core::sync::atomic::{AtomicU64, Ordering};

use spinning_top::Spinlock;
use x86_64::{
    VirtAddr,
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
        Size4KiB,
    },
};

use crate::{KernelError, memory::pmm};

static VMM: Spinlock<Option<OffsetPageTable<'static>>> = Spinlock::new(None);
static PHYSICAL_MEMORY_OFFSET: AtomicU64 = AtomicU64::new(0);
static KERNEL_CR3: AtomicU64 = AtomicU64::new(0);
pub static mut KERNEL_CR3_STATIC: u64 = 0;

pub fn init(physical_memory_offset: VirtAddr) -> Result<(), KernelError> {
    let kernel_cr3 = Cr3::read().0.start_address().as_u64();
    KERNEL_CR3.store(kernel_cr3, Ordering::Release);
    unsafe {
        KERNEL_CR3_STATIC = kernel_cr3;
    }
    let l4_table = unsafe { active_level_4_table(physical_memory_offset) };
    let mapper = unsafe { OffsetPageTable::new(l4_table, physical_memory_offset) };
    let mut vmm = VMM.lock();
    *vmm = Some(mapper);
    PHYSICAL_MEMORY_OFFSET.store(physical_memory_offset.as_u64(), Ordering::Release);
    Ok(())
}

pub fn kernel_cr3() -> u64 {
    KERNEL_CR3.load(Ordering::Acquire)
}

pub fn map_page(
    virt: Page<Size4KiB>,
    phys: PhysFrame,
    flags: PageTableFlags,
) -> Result<(), KernelError> {
    let mut vmm = VMM.lock();
    let mapper = match vmm.as_mut() {
        Some(mapper) => mapper,
        None => return Err(KernelError::VmmNotInitialized),
    };

    let mut frame_allocator = PmmFrameAllocator;
    let flush = unsafe { mapper.map_to(virt, phys, flags, &mut frame_allocator) }
        .map_err(|_| KernelError::VmmMapFailed)?;
    flush.flush();
    Ok(())
}

pub fn is_page_mapped(virt: Page<Size4KiB>) -> Result<bool, KernelError> {
    let mut vmm = VMM.lock();
    let mapper = match vmm.as_mut() {
        Some(mapper) => mapper,
        None => return Err(KernelError::VmmNotInitialized),
    };

    Ok(mapper.translate_page(virt).is_ok())
}

pub fn unmap_page(virt: Page<Size4KiB>) -> Result<(), KernelError> {
    let mut vmm = VMM.lock();
    let mapper = match vmm.as_mut() {
        Some(mapper) => mapper,
        None => return Err(KernelError::VmmNotInitialized),
    };

    let (_frame, flush) = mapper.unmap(virt).map_err(|_| KernelError::VmmUnmapFailed)?;
    flush.flush();
    Ok(())
}

pub fn phys_to_virt_addr(phys: u64) -> Result<VirtAddr, KernelError> {
    let offset = PHYSICAL_MEMORY_OFFSET.load(Ordering::Acquire);
    if offset == 0 {
        return Err(KernelError::VmmNotInitialized);
    }
    Ok(VirtAddr::new(offset.saturating_add(phys)))
}

unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    let l4_frame = Cr3::read().0;
    let virt = physical_memory_offset + l4_frame.start_address().as_u64();
    let table: *mut PageTable = virt.as_mut_ptr();
    unsafe { &mut *table }
}

struct PmmFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for PmmFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        pmm::alloc_frame().ok()
    }
}

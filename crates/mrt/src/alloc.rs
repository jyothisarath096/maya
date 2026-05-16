use crate::sys;

#[derive(Clone, Copy)]
pub struct Allocation {
    pub addr: usize,
    pub cap_lo: u64,
}

pub struct MrtAlloc;

impl MrtAlloc {
    pub fn alloc(size: usize, intent_class: u8) -> Option<Allocation> {
        unsafe {
            let (addr, cap_lo) = sys::syscall3(0x110, size as u64, 8, intent_class as u64);
            if addr < 0 {
                return None;
            }
            Some(Allocation {
                addr: addr as usize,
                cap_lo: cap_lo as u64,
            })
        }
    }

    pub fn free(cap_lo: u64) {
        unsafe {
            let _ = sys::syscall3(0x111, cap_lo, 0, 0);
        }
    }
}

const CAP_TABLE_SIZE: usize = 64;

#[derive(Clone, Copy)]
struct CapEntry {
    used: bool,
    addr: usize,
    cap_lo: u64,
}

static mut CAP_TABLE: [CapEntry; CAP_TABLE_SIZE] = {
    const EMPTY: CapEntry = CapEntry {
        used: false,
        addr: 0,
        cap_lo: 0,
    };
    [EMPTY; CAP_TABLE_SIZE]
};

fn cap_table_insert(addr: usize, cap_lo: u64) {
    unsafe {
        for slot in core::ptr::addr_of_mut!(CAP_TABLE).as_mut().unwrap().iter_mut() {
            if !slot.used {
                slot.used = true;
                slot.addr = addr;
                slot.cap_lo = cap_lo;
                return;
            }
        }
    }
}

fn cap_table_remove(addr: usize) -> Option<u64> {
    unsafe {
        for slot in core::ptr::addr_of_mut!(CAP_TABLE).as_mut().unwrap().iter_mut() {
            if slot.used && slot.addr == addr {
                slot.used = false;
                slot.addr = 0;
                let cap_lo = slot.cap_lo;
                slot.cap_lo = 0;
                return Some(cap_lo);
            }
        }
    }
    None
}

pub struct MayaGlobalAlloc;

unsafe impl core::alloc::GlobalAlloc for MayaGlobalAlloc {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        match MrtAlloc::alloc(layout.size(), 0) {
            Some(allocation) => {
                cap_table_insert(allocation.addr, allocation.cap_lo);
                allocation.addr as *mut u8
            }
            None => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: core::alloc::Layout) {
        if let Some(cap_lo) = cap_table_remove(ptr as usize) {
            MrtAlloc::free(cap_lo);
        }
    }
}

#[global_allocator]
static MAYA_ALLOC: MayaGlobalAlloc = MayaGlobalAlloc;

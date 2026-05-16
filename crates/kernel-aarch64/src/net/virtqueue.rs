pub const QUEUE_SIZE: usize = 16;
pub const VRING_DESC_F_NEXT: u16 = 1;
pub const VRING_DESC_F_WRITE: u16 = 2;
pub const KERNEL_PHYS_OFFSET: u64 = 0xFFFF_0000_0000_0000;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

#[repr(C)]
pub struct VirtqAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; QUEUE_SIZE],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtqUsedElem {
    pub id: u32,
    pub len: u32,
}

#[repr(C)]
pub struct VirtqUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtqUsedElem; QUEUE_SIZE],
}

pub const VQUEUE_MEM_BASE: u64 = 0x4600_0000;
pub const RX_DESC_PHYS: u64 = 0x4600_0000;
pub const RX_AVAIL_PHYS: u64 = 0x4600_1000;
pub const RX_USED_PHYS: u64 = 0x4600_2000;
pub const TX_DESC_PHYS: u64 = 0x4600_3000;
pub const TX_AVAIL_PHYS: u64 = 0x4600_4000;
pub const TX_USED_PHYS: u64 = 0x4600_5000;
pub const RX_BUF_PHYS: u64 = 0x4601_0000;
pub const TX_BUF_PHYS: u64 = 0x4602_0000;
pub const BUF_SIZE: usize = 2048;

pub const fn phys_to_virt(phys: u64) -> u64 {
    phys + KERNEL_PHYS_OFFSET
}

pub struct VirtQueue {
    pub desc_phys: u64,
    pub avail_phys: u64,
    pub used_phys: u64,
    pub last_used: u16,
    pub queue_idx: u32,
}

impl VirtQueue {
    pub const fn new(desc_phys: u64, avail_phys: u64, used_phys: u64, queue_idx: u32) -> Self {
        Self {
            desc_phys,
            avail_phys,
            used_phys,
            last_used: 0,
            queue_idx,
        }
    }
}

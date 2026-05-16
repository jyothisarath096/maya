pub const VIRTIO_MMIO_BASE: u64 = 0xFFFF_0000_0A00_0000;
pub const VIRTIO_MMIO_STRIDE: u64 = 0x200;
pub const VIRTIO_MMIO_SLOTS: usize = 32;

pub const REG_MAGIC: u64 = 0x000;
pub const REG_VERSION: u64 = 0x004;
pub const REG_DEVICE_ID: u64 = 0x008;
pub const REG_VENDOR_ID: u64 = 0x00C;
pub const REG_DEVICE_FEATURES: u64 = 0x010;
pub const REG_FEATURES_SEL: u64 = 0x014;
pub const REG_DRIVER_FEATURES: u64 = 0x020;
pub const REG_DRIVER_FEAT_SEL: u64 = 0x024;
pub const REG_QUEUE_SEL: u64 = 0x030;
pub const REG_QUEUE_NUM_MAX: u64 = 0x034;
pub const REG_QUEUE_NUM: u64 = 0x038;
pub const REG_QUEUE_READY: u64 = 0x044;
pub const REG_QUEUE_NOTIFY: u64 = 0x050;
pub const REG_INTERRUPT_STATUS: u64 = 0x060;
pub const REG_INTERRUPT_ACK: u64 = 0x064;
pub const REG_STATUS: u64 = 0x070;
pub const REG_QUEUE_DESC_LOW: u64 = 0x080;
pub const REG_QUEUE_DESC_HIGH: u64 = 0x084;
pub const REG_QUEUE_AVAIL_LOW: u64 = 0x090;
pub const REG_QUEUE_AVAIL_HIGH: u64 = 0x094;
pub const REG_QUEUE_USED_LOW: u64 = 0x0A0;
pub const REG_QUEUE_USED_HIGH: u64 = 0x0A4;
pub const REG_CONFIG_GEN: u64 = 0x0FC;
pub const REG_CONFIG: u64 = 0x100;

pub const STATUS_ACKNOWLEDGE: u32 = 1;
pub const STATUS_DRIVER: u32 = 2;
pub const STATUS_DRIVER_OK: u32 = 4;
pub const STATUS_FEATURES_OK: u32 = 8;
pub const STATUS_FAILED: u32 = 128;

pub const FEAT_MAC: u64 = 1 << 5;
pub const FEAT_STATUS: u64 = 1 << 16;
pub const FEAT_VERSION_1: u64 = 1 << 32;

pub fn mmio_read32(base: u64, offset: u64) -> u32 {
    unsafe { ((base + offset) as *const u32).read_volatile() }
}

pub fn mmio_write32(base: u64, offset: u64, val: u32) {
    unsafe { ((base + offset) as *mut u32).write_volatile(val) }
}

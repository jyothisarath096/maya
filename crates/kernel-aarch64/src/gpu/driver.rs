use crate::cap::table::RawSpinLock;
use crate::net::virtio::{
    mmio_read32, mmio_write32, REG_DEVICE_ID, REG_FEATURES_SEL, REG_DRIVER_FEATURES,
    REG_DRIVER_FEAT_SEL, REG_MAGIC, REG_QUEUE_AVAIL_HIGH, REG_QUEUE_AVAIL_LOW,
    REG_QUEUE_DESC_HIGH, REG_QUEUE_DESC_LOW, REG_QUEUE_NOTIFY, REG_QUEUE_NUM, REG_QUEUE_READY,
    REG_QUEUE_SEL, REG_QUEUE_USED_HIGH, REG_QUEUE_USED_LOW, REG_STATUS, REG_VERSION,
    STATUS_ACKNOWLEDGE, STATUS_DRIVER, STATUS_DRIVER_OK, STATUS_FEATURES_OK, VIRTIO_MMIO_BASE,
    VIRTIO_MMIO_SLOTS, VIRTIO_MMIO_STRIDE,
};
use crate::net::virtqueue::{
    phys_to_virt, VirtqAvail, VirtqDesc, VirtqUsed, VRING_DESC_F_NEXT, VRING_DESC_F_WRITE,
};

use super::virtio_gpu::{
    GpuCtrlHdr, GpuMemEntry, GpuRect, GpuResourceAttachBacking, GpuResourceCreate2D,
    GpuResourceFlush, GpuRespOkNodata, GpuSetScanout, GpuTransferToHost2D,
    CMD_RESOURCE_ATTACH_BACKING, CMD_RESOURCE_CREATE_2D, CMD_RESOURCE_FLUSH, CMD_SET_SCANOUT,
    CMD_TRANSFER_TO_HOST_2D, FORMAT_XRGB8888, RESP_OK_NODATA,
};

pub const GPU_MMIO_BASE: u64 = 0xFFFF_0000_0A00_0200;
pub const FB_PHYS: u64 = 0x4800_0000;
pub const FB_WIDTH: u32 = 1280;
pub const FB_HEIGHT: u32 = 800;
pub const FB_STRIDE: u32 = FB_WIDTH * 4;
pub const FB_SIZE: usize = (FB_WIDTH * FB_HEIGHT * 4) as usize;
pub const RESOURCE_ID: u32 = 1;

pub const CMD_BUF_PHYS: u64 = 0x4C00_0000;
pub const RESP_BUF_PHYS: u64 = 0x4C00_1000;
pub const GPU_DESC_PHYS: u64 = 0x4C00_2000;
pub const GPU_AVAIL_PHYS: u64 = 0x4C00_3000;
pub const GPU_USED_PHYS: u64 = 0x4C00_4000;
pub const GPU_QUEUE_SIZE: usize = 16;

pub struct GpuDriver {
    base: u64,
    initialized: bool,
    last_used: u16,
    avail_idx: u16,
    desc_head: u16,
}

impl GpuDriver {
    pub const fn new() -> Self {
        Self {
            base: GPU_MMIO_BASE,
            initialized: false,
            last_used: 0,
            avail_idx: 0,
            desc_head: 0,
        }
    }

    pub fn init(&mut self) -> bool {
        let Some(base) = self.find_gpu_device_base() else {
            crate::uart_print!("GPU: no virtio-gpu device\n");
            return false;
        };
        self.base = base;
        let magic = mmio_read32(base, REG_MAGIC);
        if magic != 0x7472_6976 {
            crate::uart_print!("GPU: no magic\n");
            return false;
        }
        let dev_id = mmio_read32(base, REG_DEVICE_ID);
        if dev_id != 16 {
            crate::uart_print!("GPU: wrong device\n");
            return false;
        }
        if mmio_read32(base, REG_VERSION) != 2 {
            crate::uart_print!("GPU: unsupported virtio version\n");
            return false;
        }

        self.zero_gpu_memory();

        mmio_write32(base, REG_STATUS, 0);
        mmio_write32(base, REG_STATUS, STATUS_ACKNOWLEDGE);
        mmio_write32(base, REG_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

        mmio_write32(base, REG_FEATURES_SEL, 0);
        mmio_write32(base, REG_DRIVER_FEATURES, 0);
        mmio_write32(base, REG_DRIVER_FEAT_SEL, 1);
        mmio_write32(base, REG_DRIVER_FEATURES, 0);

        mmio_write32(
            base,
            REG_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK,
        );

        mmio_write32(base, REG_QUEUE_SEL, 0);
        mmio_write32(base, REG_QUEUE_NUM, GPU_QUEUE_SIZE as u32);
        mmio_write32(base, REG_QUEUE_DESC_LOW, GPU_DESC_PHYS as u32);
        mmio_write32(base, REG_QUEUE_DESC_HIGH, (GPU_DESC_PHYS >> 32) as u32);
        mmio_write32(base, REG_QUEUE_AVAIL_LOW, GPU_AVAIL_PHYS as u32);
        mmio_write32(base, REG_QUEUE_AVAIL_HIGH, (GPU_AVAIL_PHYS >> 32) as u32);
        mmio_write32(base, REG_QUEUE_USED_LOW, GPU_USED_PHYS as u32);
        mmio_write32(base, REG_QUEUE_USED_HIGH, (GPU_USED_PHYS >> 32) as u32);
        mmio_write32(base, REG_QUEUE_READY, 1);

        mmio_write32(
            base,
            REG_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
        );

        self.create_resource();
        self.attach_backing();
        self.set_scanout();

        self.initialized = true;
        crate::uart_print!("GPU: initialized\n");
        true
    }

    fn find_gpu_device_base(&self) -> Option<u64> {
        for slot in 0..VIRTIO_MMIO_SLOTS {
            let base = VIRTIO_MMIO_BASE + slot as u64 * VIRTIO_MMIO_STRIDE;
            if mmio_read32(base, REG_MAGIC) != 0x7472_6976 {
                continue;
            }
            if mmio_read32(base, REG_DEVICE_ID) == 16 {
                return Some(base);
            }
        }
        None
    }

    fn zero_gpu_memory(&self) {
        unsafe {
            core::ptr::write_bytes(phys_to_virt(CMD_BUF_PHYS) as *mut u8, 0, 0x5000);
            core::ptr::write_bytes(phys_to_virt(FB_PHYS) as *mut u8, 0, FB_SIZE);
        }
    }

    fn send_cmd(&mut self, cmd_phys: u64, cmd_len: u32, resp_phys: u64, resp_len: u32) -> bool {
        let desc = unsafe {
            core::slice::from_raw_parts_mut(
                phys_to_virt(GPU_DESC_PHYS) as *mut VirtqDesc,
                GPU_QUEUE_SIZE,
            )
        };
        let d0 = (self.desc_head as usize * 2) % GPU_QUEUE_SIZE;
        let d1 = (d0 + 1) % GPU_QUEUE_SIZE;
        self.desc_head = self.desc_head.wrapping_add(1);

        desc[d0].addr = cmd_phys;
        desc[d0].len = cmd_len;
        desc[d0].flags = VRING_DESC_F_NEXT;
        desc[d0].next = d1 as u16;

        desc[d1].addr = resp_phys;
        desc[d1].len = resp_len;
        desc[d1].flags = VRING_DESC_F_WRITE;
        desc[d1].next = 0;

        let avail = unsafe { &mut *(phys_to_virt(GPU_AVAIL_PHYS) as *mut VirtqAvail) };
        let idx = self.avail_idx as usize % GPU_QUEUE_SIZE;
        avail.ring[idx] = d0 as u16;

        unsafe {
            core::arch::asm!("dsb sy", options(nomem, nostack));
        }
        avail.idx = avail.idx.wrapping_add(1);
        self.avail_idx = self.avail_idx.wrapping_add(1);
        unsafe {
            core::arch::asm!("dsb sy", options(nomem, nostack));
        }

        mmio_write32(self.base, REG_QUEUE_NOTIFY, 0);

        let used = unsafe { &*(phys_to_virt(GPU_USED_PHYS) as *const VirtqUsed) };
        let target = self.last_used.wrapping_add(1);
        let mut timeout = 10_000_000u32;
        loop {
            unsafe {
                core::arch::asm!("dsb sy", "isb", options(nomem, nostack));
            }
            if used.idx == target || timeout == 0 {
                break;
            }
            timeout -= 1;
        }
        self.last_used = target;

        let resp = unsafe { &*(phys_to_virt(resp_phys) as *const GpuRespOkNodata) };
        if resp.hdr.cmd_type != RESP_OK_NODATA {
            crate::uart_print!("GPU: bad response\n");
            return false;
        }
        true
    }

    fn create_resource(&mut self) {
        let cmd = unsafe { &mut *(phys_to_virt(CMD_BUF_PHYS) as *mut GpuResourceCreate2D) };
        *cmd = GpuResourceCreate2D {
            hdr: GpuCtrlHdr::new(CMD_RESOURCE_CREATE_2D),
            resource_id: RESOURCE_ID,
            format: FORMAT_XRGB8888,
            width: FB_WIDTH,
            height: FB_HEIGHT,
        };
        let resp = unsafe { &mut *(phys_to_virt(RESP_BUF_PHYS) as *mut GpuRespOkNodata) };
        *resp = GpuRespOkNodata {
            hdr: GpuCtrlHdr::new(0),
        };
        let _ = self.send_cmd(
            CMD_BUF_PHYS,
            core::mem::size_of::<GpuResourceCreate2D>() as u32,
            RESP_BUF_PHYS,
            core::mem::size_of::<GpuRespOkNodata>() as u32,
        );
    }

    fn attach_backing(&mut self) {
        #[repr(C)]
        struct AttachCmd {
            hdr: GpuResourceAttachBacking,
            entry: GpuMemEntry,
        }

        for _ in 0..10_000 {
            unsafe {
                core::arch::asm!("nop", options(nomem, nostack));
            }
        }

        let cmd = unsafe { &mut *(phys_to_virt(CMD_BUF_PHYS) as *mut AttachCmd) };
        cmd.hdr = GpuResourceAttachBacking {
            hdr: GpuCtrlHdr::new(CMD_RESOURCE_ATTACH_BACKING),
            resource_id: RESOURCE_ID,
            nr_entries: 1,
        };
        cmd.entry = GpuMemEntry {
            addr: FB_PHYS,
            length: FB_SIZE as u32,
            _pad: 0,
        };
        let resp = unsafe { &mut *(phys_to_virt(RESP_BUF_PHYS) as *mut GpuRespOkNodata) };
        *resp = GpuRespOkNodata {
            hdr: GpuCtrlHdr::new(0),
        };
        let _ = self.send_cmd(
            CMD_BUF_PHYS,
            core::mem::size_of::<AttachCmd>() as u32,
            RESP_BUF_PHYS,
            core::mem::size_of::<GpuRespOkNodata>() as u32,
        );
    }

    fn set_scanout(&mut self) {
        let cmd = unsafe { &mut *(phys_to_virt(CMD_BUF_PHYS) as *mut GpuSetScanout) };
        *cmd = GpuSetScanout {
            hdr: GpuCtrlHdr::new(CMD_SET_SCANOUT),
            r: GpuRect {
                x: 0,
                y: 0,
                width: FB_WIDTH,
                height: FB_HEIGHT,
            },
            scanout_id: 0,
            resource_id: RESOURCE_ID,
        };
        let resp = unsafe { &mut *(phys_to_virt(RESP_BUF_PHYS) as *mut GpuRespOkNodata) };
        *resp = GpuRespOkNodata {
            hdr: GpuCtrlHdr::new(0),
        };
        let _ = self.send_cmd(
            CMD_BUF_PHYS,
            core::mem::size_of::<GpuSetScanout>() as u32,
            RESP_BUF_PHYS,
            core::mem::size_of::<GpuRespOkNodata>() as u32,
        );
    }

    pub fn flush(&mut self, x: u32, y: u32, w: u32, h: u32) {
        if !self.initialized {
            return;
        }

        let cmd = unsafe { &mut *(phys_to_virt(CMD_BUF_PHYS) as *mut GpuTransferToHost2D) };
        *cmd = GpuTransferToHost2D {
            hdr: GpuCtrlHdr::new(CMD_TRANSFER_TO_HOST_2D),
            r: GpuRect {
                x,
                y,
                width: w,
                height: h,
            },
            offset: (y * FB_STRIDE + x * 4) as u64,
            resource_id: RESOURCE_ID,
            _pad: 0,
        };
        let resp = unsafe { &mut *(phys_to_virt(RESP_BUF_PHYS) as *mut GpuRespOkNodata) };
        *resp = GpuRespOkNodata {
            hdr: GpuCtrlHdr::new(0),
        };
        if !self.send_cmd(
            CMD_BUF_PHYS,
            core::mem::size_of::<GpuTransferToHost2D>() as u32,
            RESP_BUF_PHYS,
            core::mem::size_of::<GpuRespOkNodata>() as u32,
        ) {
            return;
        }

        let cmd2 = unsafe { &mut *(phys_to_virt(CMD_BUF_PHYS) as *mut GpuResourceFlush) };
        *cmd2 = GpuResourceFlush {
            hdr: GpuCtrlHdr::new(CMD_RESOURCE_FLUSH),
            r: GpuRect {
                x,
                y,
                width: w,
                height: h,
            },
            resource_id: RESOURCE_ID,
            _pad: 0,
        };
        *resp = GpuRespOkNodata {
            hdr: GpuCtrlHdr::new(0),
        };
        let _ = self.send_cmd(
            CMD_BUF_PHYS,
            core::mem::size_of::<GpuResourceFlush>() as u32,
            RESP_BUF_PHYS,
            core::mem::size_of::<GpuRespOkNodata>() as u32,
        );
    }

    #[inline]
    pub fn set_pixel(&self, x: u32, y: u32, r: u8, g: u8, b: u8) {
        if x >= FB_WIDTH || y >= FB_HEIGHT {
            return;
        }
        let offset = (y * FB_STRIDE + x * 4) as usize;
        let fb = unsafe {
            core::slice::from_raw_parts_mut(phys_to_virt(FB_PHYS) as *mut u8, FB_SIZE)
        };
        fb[offset] = b;
        fb[offset + 1] = g;
        fb[offset + 2] = r;
        fb[offset + 3] = 0;
    }

    pub fn clear(&self, r: u8, g: u8, b: u8) {
        let fb = unsafe {
            core::slice::from_raw_parts_mut(phys_to_virt(FB_PHYS) as *mut u8, FB_SIZE)
        };
        let mut i = 0;
        while i < FB_SIZE {
            fb[i] = b;
            fb[i + 1] = g;
            fb[i + 2] = r;
            fb[i + 3] = 0;
            i += 4;
        }
    }

    pub fn is_ready(&self) -> bool {
        self.initialized
    }
}

static GPU: RawSpinLock<GpuDriver> = RawSpinLock::new(GpuDriver::new());

pub fn init() -> bool {
    GPU.lock().init()
}

pub fn clear(r: u8, g: u8, b: u8) {
    let mut gpu = GPU.lock();
    if !gpu.is_ready() {
        return;
    }
    gpu.clear(r, g, b);
    gpu.flush(0, 0, FB_WIDTH, FB_HEIGHT);
}

pub fn set_pixel(x: u32, y: u32, r: u8, g: u8, b: u8) {
    let gpu = GPU.lock();
    if !gpu.is_ready() {
        return;
    }
    gpu.set_pixel(x, y, r, g, b);
}

pub fn set_pixel_raw(x: u32, y: u32, r: u8, g: u8, b: u8) {
    if x >= FB_WIDTH || y >= FB_HEIGHT {
        return;
    }
    let offset = (y * FB_STRIDE + x * 4) as usize;
    let fb = unsafe {
        core::slice::from_raw_parts_mut(phys_to_virt(FB_PHYS) as *mut u8, FB_SIZE)
    };
    fb[offset] = b;
    fb[offset + 1] = g;
    fb[offset + 2] = r;
    fb[offset + 3] = 0;
}

pub fn flush_region(x: u32, y: u32, w: u32, h: u32) {
    let mut gpu = GPU.lock();
    gpu.flush(x, y, w, h);
}

pub fn flush_all() {
    let mut gpu = GPU.lock();
    gpu.flush(0, 0, FB_WIDTH, FB_HEIGHT);
}

use crate::cap::table::RawSpinLock;

use super::virtio::*;
use super::virtqueue::*;

pub struct NetDriver {
    base: u64,
    mac: [u8; 6],
    rx_q: VirtQueue,
    tx_q: VirtQueue,
    initialized: bool,
}

impl NetDriver {
    pub const fn new() -> Self {
        Self {
            base: 0,
            mac: [0; 6],
            rx_q: VirtQueue::new(RX_DESC_PHYS, RX_AVAIL_PHYS, RX_USED_PHYS, 0),
            tx_q: VirtQueue::new(TX_DESC_PHYS, TX_AVAIL_PHYS, TX_USED_PHYS, 1),
            initialized: false,
        }
    }

    pub fn init(&mut self) -> bool {
        let Some(base) = self.find_net_device_base() else {
            crate::uart_print!("NET: no virtio-net device\n");
            return false;
        };
        self.base = base;

        let version = mmio_read32(self.base, REG_VERSION);
        if version != 2 {
            crate::uart_print!("NET: unsupported virtio version\n");
            return false;
        }

        self.zero_queue_memory();

        mmio_write32(self.base, REG_STATUS, 0);
        mmio_write32(self.base, REG_STATUS, STATUS_ACKNOWLEDGE);
        mmio_write32(self.base, REG_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

        mmio_write32(self.base, REG_FEATURES_SEL, 0);
        let offered_lo = mmio_read32(self.base, REG_DEVICE_FEATURES) as u64;
        mmio_write32(self.base, REG_FEATURES_SEL, 1);
        let offered_hi = mmio_read32(self.base, REG_DEVICE_FEATURES) as u64;
        let offered = offered_lo | (offered_hi << 32);
        let desired = offered & (FEAT_MAC | FEAT_STATUS | FEAT_VERSION_1);

        mmio_write32(self.base, REG_DRIVER_FEAT_SEL, 0);
        mmio_write32(self.base, REG_DRIVER_FEATURES, desired as u32);
        mmio_write32(self.base, REG_DRIVER_FEAT_SEL, 1);
        mmio_write32(self.base, REG_DRIVER_FEATURES, (desired >> 32) as u32);

        mmio_write32(
            self.base,
            REG_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK,
        );
        let status = mmio_read32(self.base, REG_STATUS);
        if (status & STATUS_FEATURES_OK) == 0 {
            mmio_write32(self.base, REG_STATUS, status | STATUS_FAILED);
            crate::uart_print!("NET: feature negotiation failed\n");
            return false;
        }

        if !self.setup_queue(self.rx_q.queue_idx, self.rx_q.desc_phys, self.rx_q.avail_phys, self.rx_q.used_phys) {
            return false;
        }
        if !self.setup_queue(self.tx_q.queue_idx, self.tx_q.desc_phys, self.tx_q.avail_phys, self.tx_q.used_phys) {
            return false;
        }

        mmio_write32(
            self.base,
            REG_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
        );

        for i in 0..6 {
            self.mac[i] = unsafe { ((self.base + REG_CONFIG + i as u64) as *const u8).read_volatile() };
        }

        self.fill_rx_queue();
        self.initialized = true;
        true
    }

    fn find_net_device_base(&self) -> Option<u64> {
        for slot in 0..VIRTIO_MMIO_SLOTS {
            let base = VIRTIO_MMIO_BASE + slot as u64 * VIRTIO_MMIO_STRIDE;
            let magic = mmio_read32(base, REG_MAGIC);
            if magic != 0x7472_6976 {
                continue;
            }
            let device_id = mmio_read32(base, REG_DEVICE_ID);
            if device_id == 1 {
                return Some(base);
            }
        }
        None
    }

    fn zero_queue_memory(&self) {
        unsafe {
            core::ptr::write_bytes(phys_to_virt(VQUEUE_MEM_BASE) as *mut u8, 0, 0x30_000);
        }
    }

    fn setup_queue(&self, idx: u32, desc_phys: u64, avail_phys: u64, used_phys: u64) -> bool {
        mmio_write32(self.base, REG_QUEUE_SEL, idx);
        let max = mmio_read32(self.base, REG_QUEUE_NUM_MAX);
        if max == 0 {
            crate::uart_print!("NET: queue unavailable\n");
            return false;
        }
        let qsz = (QUEUE_SIZE as u32).min(max);
        mmio_write32(self.base, REG_QUEUE_NUM, qsz);
        mmio_write32(self.base, REG_QUEUE_DESC_LOW, desc_phys as u32);
        mmio_write32(self.base, REG_QUEUE_DESC_HIGH, (desc_phys >> 32) as u32);
        mmio_write32(self.base, REG_QUEUE_AVAIL_LOW, avail_phys as u32);
        mmio_write32(self.base, REG_QUEUE_AVAIL_HIGH, (avail_phys >> 32) as u32);
        mmio_write32(self.base, REG_QUEUE_USED_LOW, used_phys as u32);
        mmio_write32(self.base, REG_QUEUE_USED_HIGH, (used_phys >> 32) as u32);
        mmio_write32(self.base, REG_QUEUE_READY, 1);
        true
    }

    fn fill_rx_queue(&mut self) {
        let avail = unsafe { &mut *(phys_to_virt(self.rx_q.avail_phys) as *mut VirtqAvail) };
        let desc = unsafe {
            core::slice::from_raw_parts_mut(
                phys_to_virt(self.rx_q.desc_phys) as *mut VirtqDesc,
                QUEUE_SIZE,
            )
        };
        for i in 0..QUEUE_SIZE {
            let buf_phys = RX_BUF_PHYS + (i * BUF_SIZE) as u64;
            desc[i].addr = buf_phys;
            desc[i].len = BUF_SIZE as u32;
            desc[i].flags = VRING_DESC_F_WRITE;
            desc[i].next = 0;
            avail.ring[i] = i as u16;
        }
        unsafe {
            core::arch::asm!("dsb sy", options(nomem, nostack));
        }
        avail.idx = QUEUE_SIZE as u16;
        mmio_write32(self.base, REG_QUEUE_NOTIFY, self.rx_q.queue_idx);
    }

    pub fn send(&mut self, data: &[u8]) -> bool {
        if !self.initialized || data.len() > BUF_SIZE - 12 {
            return false;
        }

        let buf_phys = TX_BUF_PHYS;
        let buf = unsafe {
            core::slice::from_raw_parts_mut(phys_to_virt(buf_phys) as *mut u8, BUF_SIZE)
        };
        for b in buf.iter_mut().take(12) {
            *b = 0;
        }
        let plen = data.len();
        buf[12..12 + plen].copy_from_slice(data);

        let desc = unsafe { &mut *(phys_to_virt(self.tx_q.desc_phys) as *mut VirtqDesc) };
        desc.addr = buf_phys;
        desc.len = (12 + plen) as u32;
        desc.flags = 0;
        desc.next = 0;

        let avail = unsafe { &mut *(phys_to_virt(self.tx_q.avail_phys) as *mut VirtqAvail) };
        let idx = avail.idx as usize % QUEUE_SIZE;
        avail.ring[idx] = 0;
        unsafe {
            core::arch::asm!("dsb sy", options(nomem, nostack));
        }
        avail.idx = avail.idx.wrapping_add(1);
        unsafe {
            core::arch::asm!("dsb sy", options(nomem, nostack));
        }
        mmio_write32(self.base, REG_QUEUE_NOTIFY, self.tx_q.queue_idx);
        true
    }

    pub fn recv(&mut self, out: &mut [u8]) -> usize {
        if !self.initialized {
            return 0;
        }
        let used = unsafe { &*(phys_to_virt(self.rx_q.used_phys) as *const VirtqUsed) };
        let last = self.rx_q.last_used;
        if used.idx == last {
            return 0;
        }
        let elem = &used.ring[last as usize % QUEUE_SIZE];
        let desc_id = elem.id as usize;
        let len = elem.len as usize;
        let buf_phys = RX_BUF_PHYS + (desc_id * BUF_SIZE) as u64;
        let buf = unsafe {
            core::slice::from_raw_parts(phys_to_virt(buf_phys) as *const u8, len.min(BUF_SIZE))
        };
        let data_len = if len > 12 { (len - 12).min(out.len()) } else { 0 };
        if data_len > 0 {
            out[..data_len].copy_from_slice(&buf[12..12 + data_len]);
        }
        self.rx_q.last_used = last.wrapping_add(1);

        let avail = unsafe { &mut *(phys_to_virt(self.rx_q.avail_phys) as *mut VirtqAvail) };
        let idx = avail.idx as usize % QUEUE_SIZE;
        avail.ring[idx] = desc_id as u16;
        unsafe {
            core::arch::asm!("dsb sy", options(nomem, nostack));
        }
        avail.idx = avail.idx.wrapping_add(1);
        mmio_write32(self.base, REG_QUEUE_NOTIFY, self.rx_q.queue_idx);
        data_len
    }

    pub fn mac(&self) -> [u8; 6] {
        self.mac
    }
}

static NET_DRIVER: RawSpinLock<NetDriver> = RawSpinLock::new(NetDriver::new());

pub fn init() -> bool {
    NET_DRIVER.lock().init()
}

pub fn send_packet(data: &[u8]) -> bool {
    NET_DRIVER.lock().send(data)
}

pub fn recv_packet(out: &mut [u8]) -> usize {
    NET_DRIVER.lock().recv(out)
}

pub fn get_mac() -> [u8; 6] {
    NET_DRIVER.lock().mac()
}

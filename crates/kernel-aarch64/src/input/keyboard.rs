use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use crate::cap::table::RawSpinLock;
use crate::net::virtio::*;
use crate::net::virtqueue::{phys_to_virt, VirtQueue, VirtqAvail, VirtqDesc, VirtqUsed, VRING_DESC_F_WRITE};

pub const KB_BUF_PHYS: u64 = 0x4D00_0000;
pub const KB_DESC_PHYS: u64 = 0x4D00_1000;
pub const KB_AVAIL_PHYS: u64 = 0x4D00_2000;
pub const KB_USED_PHYS: u64 = 0x4D00_3000;
pub const KB_QUEUE_SIZE: usize = 16;

pub const EV_KEY: u16 = 1;
pub const KEY_PRESS: u32 = 1;
pub const KEY_RELEASE: u32 = 0;
pub const KEY_REPEAT: u32 = 2;
const DEVICE_ID_INPUT: u32 = 18;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct InputEvent {
    pub ev_type: u16,
    pub code: u16,
    pub value: u32,
}

static KEY_RING: [AtomicU8; 64] = [const { AtomicU8::new(0) }; 64];
static KEY_HEAD: AtomicU32 = AtomicU32::new(0);
static KEY_TAIL: AtomicU32 = AtomicU32::new(0);
static SHIFT_HELD: AtomicBool = AtomicBool::new(false);
static CTRL_HELD: AtomicBool = AtomicBool::new(false);

pub struct KeyboardDriver {
    base: u64,
    queue: VirtQueue,
    initialized: bool,
}

impl KeyboardDriver {
    pub const fn new() -> Self {
        Self {
            base: 0,
            queue: VirtQueue::new(KB_DESC_PHYS, KB_AVAIL_PHYS, KB_USED_PHYS, 0),
            initialized: false,
        }
    }

    pub fn init(&mut self) -> bool {
        let Some(base) = self.find_keyboard_device_base() else {
            return false;
        };
        self.base = base;

        if mmio_read32(self.base, REG_VERSION) != 2 {
            return false;
        }

        unsafe {
            core::ptr::write_bytes(phys_to_virt(KB_BUF_PHYS) as *mut u8, 0, 0x4000);
        }

        mmio_write32(self.base, REG_STATUS, 0);
        mmio_write32(self.base, REG_STATUS, STATUS_ACKNOWLEDGE);
        mmio_write32(self.base, REG_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);

        mmio_write32(self.base, REG_DRIVER_FEAT_SEL, 0);
        mmio_write32(self.base, REG_DRIVER_FEATURES, 0);
        mmio_write32(self.base, REG_DRIVER_FEAT_SEL, 1);
        mmio_write32(self.base, REG_DRIVER_FEATURES, 0);
        mmio_write32(self.base, REG_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK);
        if (mmio_read32(self.base, REG_STATUS) & STATUS_FEATURES_OK) == 0 {
            return false;
        }

        if !self.setup_queue() {
            return false;
        }
        self.fill_event_queue();

        mmio_write32(
            self.base,
            REG_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
        );
        self.initialized = true;
        true
    }

    fn find_keyboard_device_base(&self) -> Option<u64> {
        for slot in 0..VIRTIO_MMIO_SLOTS {
            let base = VIRTIO_MMIO_BASE + slot as u64 * VIRTIO_MMIO_STRIDE;
            if mmio_read32(base, REG_MAGIC) != 0x7472_6976 {
                continue;
            }
            if mmio_read32(base, REG_DEVICE_ID) == DEVICE_ID_INPUT {
                return Some(base);
            }
        }
        None
    }

    fn setup_queue(&self) -> bool {
        mmio_write32(self.base, REG_QUEUE_SEL, self.queue.queue_idx);
        let max = mmio_read32(self.base, REG_QUEUE_NUM_MAX);
        if max == 0 {
            return false;
        }
        let qsz = (KB_QUEUE_SIZE as u32).min(max);
        mmio_write32(self.base, REG_QUEUE_NUM, qsz);
        mmio_write32(self.base, REG_QUEUE_DESC_LOW, self.queue.desc_phys as u32);
        mmio_write32(self.base, REG_QUEUE_DESC_HIGH, (self.queue.desc_phys >> 32) as u32);
        mmio_write32(self.base, REG_QUEUE_AVAIL_LOW, self.queue.avail_phys as u32);
        mmio_write32(self.base, REG_QUEUE_AVAIL_HIGH, (self.queue.avail_phys >> 32) as u32);
        mmio_write32(self.base, REG_QUEUE_USED_LOW, self.queue.used_phys as u32);
        mmio_write32(self.base, REG_QUEUE_USED_HIGH, (self.queue.used_phys >> 32) as u32);
        mmio_write32(self.base, REG_QUEUE_READY, 1);
        true
    }

    fn fill_event_queue(&mut self) {
        let avail = unsafe { &mut *(phys_to_virt(self.queue.avail_phys) as *mut VirtqAvail) };
        let desc = unsafe {
            core::slice::from_raw_parts_mut(phys_to_virt(self.queue.desc_phys) as *mut VirtqDesc, KB_QUEUE_SIZE)
        };
        for i in 0..KB_QUEUE_SIZE {
            let buf_phys = KB_BUF_PHYS + (i * core::mem::size_of::<InputEvent>()) as u64;
            desc[i].addr = buf_phys;
            desc[i].len = core::mem::size_of::<InputEvent>() as u32;
            desc[i].flags = VRING_DESC_F_WRITE;
            desc[i].next = 0;
            avail.ring[i] = i as u16;
        }
        unsafe {
            core::arch::asm!("dsb sy", options(nomem, nostack));
        }
        avail.idx = KB_QUEUE_SIZE as u16;
        mmio_write32(self.base, REG_QUEUE_NOTIFY, self.queue.queue_idx);
    }

    pub fn poll(&mut self) {
        if !self.initialized {
            return;
        }
        let used = unsafe { &*(phys_to_virt(self.queue.used_phys) as *const VirtqUsed) };
        while self.queue.last_used != used.idx {
            let elem = &used.ring[self.queue.last_used as usize % KB_QUEUE_SIZE];
            let desc_id = elem.id as usize;
            let buf_phys = KB_BUF_PHYS + (desc_id * core::mem::size_of::<InputEvent>()) as u64;
            let event = unsafe { *(phys_to_virt(buf_phys) as *const InputEvent) };
            handle_event(event);
            self.queue.last_used = self.queue.last_used.wrapping_add(1);

            let avail = unsafe { &mut *(phys_to_virt(self.queue.avail_phys) as *mut VirtqAvail) };
            let idx = avail.idx as usize % KB_QUEUE_SIZE;
            avail.ring[idx] = desc_id as u16;
            unsafe {
                core::arch::asm!("dsb sy", options(nomem, nostack));
            }
            avail.idx = avail.idx.wrapping_add(1);
            mmio_write32(self.base, REG_QUEUE_NOTIFY, self.queue.queue_idx);
        }
    }
}

static KEYBOARD: RawSpinLock<KeyboardDriver> = RawSpinLock::new(KeyboardDriver::new());

pub fn init() -> bool {
    KEYBOARD.lock().init()
}

pub fn poll() {
    KEYBOARD.lock().poll();
}

pub fn key_available() -> bool {
    KEY_HEAD.load(Ordering::Acquire) != KEY_TAIL.load(Ordering::Acquire)
}

pub fn read_key() -> Option<u8> {
    let tail = KEY_TAIL.load(Ordering::Acquire);
    let head = KEY_HEAD.load(Ordering::Acquire);
    if tail == head {
        return None;
    }
    let idx = (tail as usize) % KEY_RING.len();
    let ch = KEY_RING[idx].load(Ordering::Relaxed);
    KEY_TAIL.store(tail.wrapping_add(1), Ordering::Release);
    Some(ch)
}

fn push_key(ch: u8) {
    let head = KEY_HEAD.load(Ordering::Acquire);
    let tail = KEY_TAIL.load(Ordering::Acquire);
    let next = head.wrapping_add(1);
    if next.wrapping_sub(tail) > KEY_RING.len() as u32 {
        return;
    }
    let idx = (head as usize) % KEY_RING.len();
    KEY_RING[idx].store(ch, Ordering::Relaxed);
    KEY_HEAD.store(next, Ordering::Release);
}

fn handle_event(event: InputEvent) {
    if event.ev_type != EV_KEY {
        return;
    }
    match event.code {
        225 | 229 => {
            SHIFT_HELD.store(event.value != KEY_RELEASE, Ordering::Relaxed);
            return;
        }
        224 | 228 => {
            CTRL_HELD.store(event.value != KEY_RELEASE, Ordering::Relaxed);
            return;
        }
        _ => {}
    }
    if event.value != KEY_PRESS && event.value != KEY_REPEAT {
        return;
    }
    if CTRL_HELD.load(Ordering::Relaxed) && event.code == 43 {
        return;
    }
    if let Some(ch) = hid_to_ascii(event.code, SHIFT_HELD.load(Ordering::Relaxed)) {
        push_key(ch);
    }
}

fn hid_to_ascii(code: u16, shift: bool) -> Option<u8> {
    let ch = match code {
        4..=29 => {
            let base = if shift { b'A' } else { b'a' };
            base + (code as u8 - 4)
        }
        30 => if shift { b'!' } else { b'1' },
        31 => if shift { b'@' } else { b'2' },
        32 => if shift { b'#' } else { b'3' },
        33 => if shift { b'$' } else { b'4' },
        34 => if shift { b'%' } else { b'5' },
        35 => if shift { b'^' } else { b'6' },
        36 => if shift { b'&' } else { b'7' },
        37 => if shift { b'*' } else { b'8' },
        38 => if shift { b'(' } else { b'9' },
        39 => if shift { b')' } else { b'0' },
        40 => b'\n',
        42 => 0x08,
        43 => b'\t',
        44 => b' ',
        45 => if shift { b'_' } else { b'-' },
        46 => if shift { b'+' } else { b'=' },
        47 => if shift { b'{' } else { b'[' },
        48 => if shift { b'}' } else { b']' },
        49 => if shift { b'|' } else { b'\\' },
        51 => if shift { b':' } else { b';' },
        52 => if shift { b'"' } else { b'\'' },
        53 => if shift { b'~' } else { b'`' },
        54 => if shift { b'<' } else { b',' },
        55 => if shift { b'>' } else { b'.' },
        56 => if shift { b'?' } else { b'/' },
        _ => return None,
    };
    Some(ch)
}

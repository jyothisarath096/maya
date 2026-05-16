const FRAME_SIZE: usize = 4096;
const MAX_FRAMES: usize = 524288;
const PHYS_TO_VIRT_OFFSET: u64 = 0xFFFF_0000_0000_0000;

static mut BITMAP: [u64; MAX_FRAMES / 64] = [0u64; MAX_FRAMES / 64];
static mut TOTAL_FRAMES: usize = 0;
static mut FREE_FRAMES: usize = 0;
static mut PHYS_OFFSET: u64 = 0;

pub fn init() {
    unsafe {
        PHYS_OFFSET = PHYS_TO_VIRT_OFFSET;
    }
    for i in 0..MAX_FRAMES / 64 {
        unsafe {
            BITMAP[i] = u64::MAX;
        }
    }

    let ram_start = 0x4100_0000usize;
    let ram_end = 0x47F0_0000usize;
    for frame in (ram_start..ram_end).step_by(FRAME_SIZE) {
        mark_frame_free(frame / FRAME_SIZE);
    }

    crate::uart_print!("PMM initialised\n");
}

pub fn alloc_frame() -> Option<u64> {
    unsafe {
        for i in 0..MAX_FRAMES / 64 {
            if BITMAP[i] != u64::MAX {
                let bit = BITMAP[i].trailing_ones() as usize;
                BITMAP[i] |= 1 << bit;
                FREE_FRAMES = FREE_FRAMES.saturating_sub(1);
                let frame = (i * 64 + bit) as u64 * FRAME_SIZE as u64;
                return Some(frame);
            }
        }
        None
    }
}

fn mark_frame_free(frame: usize) {
    if frame >= MAX_FRAMES {
        return;
    }
    unsafe {
        let word = frame / 64;
        let bit = frame % 64;
        BITMAP[word] &= !(1u64 << bit);
        FREE_FRAMES = FREE_FRAMES.saturating_add(1);
        TOTAL_FRAMES = TOTAL_FRAMES.saturating_add(1);
    }
}

pub fn free_frame(phys: u64) {
    let frame = (phys as usize) / FRAME_SIZE;
    mark_frame_free(frame);
}

pub fn phys_to_virt(phys: u64) -> u64 {
    unsafe { phys.wrapping_add(PHYS_OFFSET) }
}

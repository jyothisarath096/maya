#![allow(dead_code)]

use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use spinning_top::Spinlock;
use x86_64::{
    PhysAddr,
    structures::paging::PhysFrame,
};

use crate::KernelError;

const FRAME_SIZE: u64 = 4096;
const LEGACY_RESERVED_FRAMES: usize = 256;
const MAX_FRAMES: usize = 512 * 1024;
const BITMAP_SIZE: usize = MAX_FRAMES / 8;

pub struct PmmStats {
    pub total_frames: usize,
    pub used_frames: usize,
    pub free_frames: usize,
}

struct PmmState {
    total_frames: usize,
    used_frames: usize,
    initialized: bool,
}

static BITMAP: Spinlock<[u8; BITMAP_SIZE]> = Spinlock::new([0xFF; BITMAP_SIZE]);
static PMM_STATE: Spinlock<PmmState> = Spinlock::new(PmmState {
    total_frames: 0,
    used_frames: 0,
    initialized: false,
});

pub fn init(memory_map: &'static MemoryRegions) {
    let mut highest_addr = 0u64;

    for region in memory_map.iter() {
        if region.end > highest_addr {
            highest_addr = region.end;
        }
    }

    let total_frames = (highest_addr.div_ceil(FRAME_SIZE) as usize).min(MAX_FRAMES);
    let mut bitmap = BITMAP.lock();
    for byte in bitmap.iter_mut() {
        *byte = 0xFF;
    }

    let mut used_frames = total_frames;

    for region in memory_map.iter() {
        if region.kind == MemoryRegionKind::Usable {
            let start_frame = (align_up(region.start, FRAME_SIZE) / FRAME_SIZE) as usize;
            let end_frame = (align_down(region.end, FRAME_SIZE) / FRAME_SIZE) as usize;
            for frame in start_frame..end_frame {
                if frame < total_frames && mark_frame_free_raw(&mut *bitmap, frame) {
                    used_frames -= 1;
                }
            }
        }
    }

    for frame in 0..LEGACY_RESERVED_FRAMES.min(total_frames) {
        if mark_frame_used_raw(&mut *bitmap, frame) {
            used_frames += 1;
        }
    }
    drop(bitmap);

    let mut state = PMM_STATE.lock();
    state.total_frames = total_frames;
    state.used_frames = used_frames;
    state.initialized = total_frames > 0;
}

pub fn alloc_frame() -> Result<PhysFrame, KernelError> {
    let mut state = PMM_STATE.lock();
    if !state.initialized {
        return Err(KernelError::PmmUninitialized);
    }

    let mut bitmap = BITMAP.lock();
    for frame in LEGACY_RESERVED_FRAMES.min(state.total_frames)..state.total_frames {
        if mark_frame_used_raw(&mut *bitmap, frame) {
            state.used_frames += 1;
            let addr = PhysAddr::new((frame as u64) * FRAME_SIZE);
            return PhysFrame::from_start_address(addr).map_err(|_| KernelError::PmmInvalidFrame);
        }
    }

    Err(KernelError::PmmOutOfMemory)
}

pub fn free_frame(frame: PhysFrame) -> Result<(), KernelError> {
    let mut state = PMM_STATE.lock();
    if !state.initialized {
        return Err(KernelError::PmmUninitialized);
    }

    let frame_index = (frame.start_address().as_u64() / FRAME_SIZE) as usize;
    if frame_index >= state.total_frames {
        return Err(KernelError::PmmInvalidFrame);
    }

    let mut bitmap = BITMAP.lock();
    if !mark_frame_free_raw(&mut *bitmap, frame_index) {
        return Err(KernelError::PmmDoubleFree);
    }

    state.used_frames -= 1;
    Ok(())
}

pub fn stats() -> PmmStats {
    let state = PMM_STATE.lock();
    let free_frames = state.total_frames.saturating_sub(state.used_frames);
    PmmStats {
        total_frames: state.total_frames,
        used_frames: state.used_frames,
        free_frames,
    }
}

fn align_up(value: u64, align: u64) -> u64 {
    value.div_ceil(align) * align
}

fn align_down(value: u64, align: u64) -> u64 {
    (value / align) * align
}

fn mark_frame_used_raw(bitmap: &mut [u8; BITMAP_SIZE], frame: usize) -> bool {
    let byte_index = frame / 8;
    let bit_mask = 1u8 << (frame % 8);
    let current = bitmap[byte_index];
    if current & bit_mask != 0 {
        false
    } else {
        bitmap[byte_index] = current | bit_mask;
        true
    }
}

fn mark_frame_free_raw(bitmap: &mut [u8; BITMAP_SIZE], frame: usize) -> bool {
    let byte_index = frame / 8;
    let bit_mask = 1u8 << (frame % 8);
    let current = bitmap[byte_index];
    if current & bit_mask == 0 {
        false
    } else {
        bitmap[byte_index] = current & !bit_mask;
        true
    }
}

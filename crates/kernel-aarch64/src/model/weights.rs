#![allow(dead_code)]

use core::sync::atomic::{AtomicBool, AtomicI32, Ordering};

pub struct ModelWeights {
    pub l1_w: &'static [i8; 2048],
    pub l1_b: &'static [i32; 128],
    pub l2_w: &'static [i8; 8192],
    pub l2_b: &'static [i32; 64],
    pub out_w: [i8; 64],
    pub out_b: [i32; 1],
}

static MODEL_WEIGHTS: &[u8] = include_bytes!("../../model_weights.bin");
static PARSED: AtomicBool = AtomicBool::new(false);

static mut L1_W_DATA: [i8; 2048] = [0; 2048];
static mut L1_B_DATA: [i32; 128] = [0; 128];
static mut L2_W_DATA: [i8; 8192] = [0; 8192];
static mut L2_B_DATA: [i32; 64] = [0; 64];
static OUT_W_DATA: [AtomicI32; 64] = [const { AtomicI32::new(0) }; 64];
static OUT_B_DATA: [AtomicI32; 1] = [const { AtomicI32::new(0) }; 1];

fn casal_update_i32(dst: &AtomicI32, update: impl Fn(i32) -> i32) {
    let ptr = dst.as_ptr();
    let mut current = dst.load(Ordering::Relaxed);
    loop {
        let next = update(current);
        let mut observed = current;
        unsafe {
            core::arch::asm!(
                "casal {old:w}, {new:w}, [{ptr}]",
                old = inout(reg) observed,
                new = in(reg) next,
                ptr = in(reg) ptr,
                options(nostack)
            );
        }
        if observed == current {
            break;
        }
        current = observed;
    }
}

fn parse_i32(offset: usize) -> i32 {
    i32::from_le_bytes([
        MODEL_WEIGHTS[offset],
        MODEL_WEIGHTS[offset + 1],
        MODEL_WEIGHTS[offset + 2],
        MODEL_WEIGHTS[offset + 3],
    ])
}

pub fn init() {
    if PARSED.load(Ordering::Acquire) {
        return;
    }

    if MODEL_WEIGHTS.len() != 11_076 {
        return;
    }

    unsafe {
        for i in 0..2048 {
            L1_W_DATA[i] = MODEL_WEIGHTS[i] as i8;
        }
        for i in 0..128 {
            let off = 2048 + i * 4;
            L1_B_DATA[i] = parse_i32(off);
        }
        for i in 0..8192 {
            L2_W_DATA[i] = MODEL_WEIGHTS[2560 + i] as i8;
        }
        for i in 0..64 {
            let off = 10_752 + i * 4;
            L2_B_DATA[i] = parse_i32(off);
        }
    }

    for i in 0..64 {
        OUT_W_DATA[i].store(MODEL_WEIGHTS[11_008 + i] as i8 as i32, Ordering::Relaxed);
    }
    OUT_B_DATA[0].store(parse_i32(11_072), Ordering::Relaxed);

    PARSED.store(true, Ordering::Release);
}

pub fn load() -> ModelWeights {
    if !PARSED.load(Ordering::Acquire) {
        init();
    }

    let mut out_w = [0i8; 64];
    let mut out_b = [0i32; 1];
    for i in 0..64 {
        out_w[i] = OUT_W_DATA[i]
            .load(Ordering::Relaxed)
            .clamp(i8::MIN as i32, i8::MAX as i32) as i8;
    }
    out_b[0] = OUT_B_DATA[0].load(Ordering::Relaxed);

    unsafe {
        ModelWeights {
            l1_w: &*core::ptr::addr_of!(L1_W_DATA),
            l1_b: &*core::ptr::addr_of!(L1_B_DATA),
            l2_w: &*core::ptr::addr_of!(L2_W_DATA),
            l2_b: &*core::ptr::addr_of!(L2_B_DATA),
            out_w,
            out_b,
        }
    }
}

pub fn all_zero() -> bool {
    let model = load();
    model.l1_w.iter().all(|&v| v == 0)
        && model.l1_b.iter().all(|&v| v == 0)
        && model.l2_w.iter().all(|&v| v == 0)
        && model.l2_b.iter().all(|&v| v == 0)
        && model.out_w.iter().all(|&v| v == 0)
        && model.out_b.iter().all(|&v| v == 0)
}

pub fn update_output_weights(h2_activations: &[f32; 64], advantage: i32) {
    if advantage == 0 {
        return;
    }

    const LR: i32 = 40;

    let grad_b = (advantage * LR) / 100;
    if grad_b != 0 {
        casal_update_i32(&OUT_B_DATA[0], |current| {
            current.saturating_add(grad_b).clamp(-32_768, 32_767)
        });
    }

    let mut best_idx = 0usize;
    let mut best_activation = 0i32;
    for (idx, &activation) in h2_activations.iter().enumerate() {
        let scaled = (activation * 1000.0) as i32;
        if scaled > best_activation {
            best_activation = scaled;
            best_idx = idx;
        }
    }

    if best_activation <= 0 {
        return;
    }

    let signed = (advantage as i64) * (best_activation as i64) * (LR as i64);
    let mut delta = (signed / 1_000_000) as i32;
    if delta == 0 {
        delta = if advantage > 0 { 1 } else { -1 };
    }
    delta = delta.clamp(-2, 2);

    casal_update_i32(&OUT_W_DATA[best_idx], |current| {
        current.saturating_add(delta).clamp(i8::MIN as i32, i8::MAX as i32)
    });
}

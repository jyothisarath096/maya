#![allow(dead_code)]

const L1_W_LEN: usize = 16 * 128;
const L1_B_LEN: usize = 128;
const L2_W_LEN: usize = 128 * 64;
const L2_B_LEN: usize = 64;
const OUT_W_LEN: usize = 64;
const OUT_B_LEN: usize = 1;

static MODEL_WEIGHTS: &[u8] = include_bytes!("../../model_weights.bin");

static ZERO_L1_W: [i8; L1_W_LEN] = [0; L1_W_LEN];
static ZERO_L1_B: [i32; L1_B_LEN] = [0; L1_B_LEN];
static ZERO_L2_W: [i8; L2_W_LEN] = [0; L2_W_LEN];
static ZERO_L2_B: [i32; L2_B_LEN] = [0; L2_B_LEN];
static ZERO_OUT_W: [i8; OUT_W_LEN] = [0; OUT_W_LEN];
static ZERO_OUT_B: [i32; OUT_B_LEN] = [0; OUT_B_LEN];

pub struct ModelWeights {
    pub l1_w: &'static [i8],
    pub l1_b: &'static [i32],
    pub l2_w: &'static [i8],
    pub l2_b: &'static [i32],
    pub out_w: &'static [i8],
    pub out_b: &'static [i32],
}

pub fn load() -> ModelWeights {
    let expected_len = L1_W_LEN + (L1_B_LEN * 4) + L2_W_LEN + (L2_B_LEN * 4) + OUT_W_LEN + 4;
    let _valid_layout = MODEL_WEIGHTS.len() == expected_len;

    ModelWeights {
        l1_w: &ZERO_L1_W,
        l1_b: &ZERO_L1_B,
        l2_w: &ZERO_L2_W,
        l2_b: &ZERO_L2_B,
        out_w: &ZERO_OUT_W,
        out_b: &ZERO_OUT_B,
    }
}

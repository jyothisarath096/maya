#![allow(dead_code)]

use super::weights::ModelWeights;

const INPUTS: usize = 16;
const H1: usize = 128;
const H2: usize = 64;
const SCALE_FACTOR: f32 = 127.0;

#[cfg(target_arch = "aarch64")]
use core::arch::aarch64::*;

#[cfg(target_arch = "aarch64")]
unsafe fn matmul_neon(
    weights: &[i8],
    inputs: &[i8],
    biases: &[i32],
    output: &mut [f32],
    in_dim: usize,
    out_dim: usize,
) {
    for neuron in 0..out_dim {
        let mut acc = biases[neuron];
        let w_row = &weights[neuron * in_dim..][..in_dim];
        let mut i = 0usize;
        while i + 8 <= in_dim {
            let w_vec = unsafe { vld1_s8(w_row[i..].as_ptr()) };
            let x_vec = unsafe { vld1_s8(inputs[i..].as_ptr()) };
            let prod = unsafe { vmull_s8(w_vec, x_vec) };
            acc += unsafe { vaddlvq_s16(prod) } as i32;
            i += 8;
        }
        while i < in_dim {
            acc += (w_row[i] as i32) * (inputs[i] as i32);
            i += 1;
        }
        let value = (acc as f32) / (SCALE_FACTOR * SCALE_FACTOR);
        output[neuron] = value.max(0.0);
    }
}

#[cfg(not(target_arch = "aarch64"))]
unsafe fn matmul_neon(
    weights: &[i8],
    inputs: &[i8],
    biases: &[i32],
    output: &mut [f32],
    in_dim: usize,
    out_dim: usize,
) {
    for neuron in 0..out_dim {
        let mut acc = biases[neuron];
        let w_row = &weights[neuron * in_dim..][..in_dim];
        for i in 0..in_dim {
            acc += (w_row[i] as i32) * (inputs[i] as i32);
        }
        let value = (acc as f32) / (SCALE_FACTOR * SCALE_FACTOR);
        output[neuron] = value.max(0.0);
    }
}

pub fn forward(features: &[f32; 16], weights: &ModelWeights) -> f32 {
    let (score, _) = forward_with_activations(features, weights);
    score
}

pub fn forward_with_activations(features: &[f32; 16], weights: &ModelWeights) -> (f32, [f32; 64]) {
    let input_q = quantize(features);

    let mut h1 = [0.0f32; H1];
    unsafe {
        matmul_neon(weights.l1_w, &input_q, weights.l1_b, &mut h1, INPUTS, H1);
    }
    let h1_q = quantize(&h1);

    let mut h2 = [0.0f32; H2];
    unsafe {
        matmul_neon(weights.l2_w, &h1_q, weights.l2_b, &mut h2, H1, H2);
    }
    let h2_q = quantize(&h2);

    let mut acc = weights.out_b[0];
    for i in 0..H2 {
        acc += (h2_q[i] as i32) * (weights.out_w[i] as i32);
    }

    let value = (acc as f32) / (SCALE_FACTOR * SCALE_FACTOR);
    let score = sigmoid(value);

    (score, h2)
}

pub fn sigmoid(x: f32) -> f32 {
    if x >= 6.0 {
        return 1.0;
    }
    if x <= -6.0 {
        return 0.0;
    }
    1.0 / (1.0 + fast_exp(-x))
}

fn fast_exp(x: f32) -> f32 {
    let xi = (12_102_203.0_f32 * x + 1_064_986_823.0_f32) as u32;
    f32::from_bits(xi)
}

fn quantize<const N: usize>(values: &[f32; N]) -> [i8; N] {
    let mut quantized = [0i8; N];
    for (index, value) in values.iter().enumerate() {
        let scaled = (*value * SCALE_FACTOR).clamp(-128.0, 127.0);
        quantized[index] = scaled as i8;
    }
    quantized
}

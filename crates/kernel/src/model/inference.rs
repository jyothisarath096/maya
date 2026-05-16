#![allow(dead_code)]

use super::weights::ModelWeights;

const INPUTS: usize = 16;
const H1: usize = 128;
const H2: usize = 64;
const SCALE_FACTOR: f32 = 127.0;

pub fn forward(features: &[f32; 16], weights: &ModelWeights) -> f32 {
    let input_q = quantize(features);

    let mut h1 = [0.0f32; H1];
    for neuron in 0..H1 {
        let mut acc = weights.l1_b[neuron];
        for input in 0..INPUTS {
            let weight_index = neuron * INPUTS + input;
            acc += (input_q[input] as i32) * (weights.l1_w[weight_index] as i32);
        }
        let value = (acc as f32) / (SCALE_FACTOR * SCALE_FACTOR);
        h1[neuron] = value.max(0.0);
    }

    let h1_q = quantize(&h1);

    let mut h2 = [0.0f32; H2];
    for neuron in 0..H2 {
        let mut acc = weights.l2_b[neuron];
        for input in 0..H1 {
            let weight_index = neuron * H1 + input;
            acc += (h1_q[input] as i32) * (weights.l2_w[weight_index] as i32);
        }
        let value = (acc as f32) / (SCALE_FACTOR * SCALE_FACTOR);
        h2[neuron] = value.max(0.0);
    }

    let h2_q = quantize(&h2);

    let mut acc = weights.out_b[0];
    for input in 0..H2 {
        acc += (h2_q[input] as i32) * (weights.out_w[input] as i32);
    }

    let value = (acc as f32) / (SCALE_FACTOR * SCALE_FACTOR);
    sigmoid(value)
}

pub fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + libm::expf(-x))
}

fn quantize<const N: usize>(values: &[f32; N]) -> [i8; N] {
    let mut quantized = [0i8; N];

    for (index, value) in values.iter().enumerate() {
        let scaled = (*value * SCALE_FACTOR).clamp(-128.0, 127.0);
        quantized[index] = scaled as i8;
    }

    quantized
}

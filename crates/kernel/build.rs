fn main() {
    let path = "model_weights.bin";
    if !std::path::Path::new(path).exists() {
        let weights = vec![0u8; 11076];
        std::fs::write(path, weights).unwrap();
    }
    println!("cargo:rerun-if-changed=model_weights.bin");

    let hash_path = "kernel.sha256";
    if !std::path::Path::new(hash_path).exists() {
        std::fs::write(hash_path, vec![0u8; 32]).unwrap();
    }
    println!("cargo:rerun-if-changed=kernel.sha256");
}

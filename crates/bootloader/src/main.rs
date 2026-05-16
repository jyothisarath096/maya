use std::path::Path;

fn main() {
    let kernel_path = std::env::args()
        .nth(1)
        .expect("usage: bootloader <path-to-kernel-elf>");
    let kernel_path = Path::new(&kernel_path);

    let out_dir = kernel_path.parent().unwrap();
    let uefi_path = out_dir.join("uefi.img");

    bootloader::UefiBoot::new(kernel_path)
        .create_disk_image(&uefi_path)
        .expect("failed to create UEFI disk image");

    println!("UEFI image: {}", uefi_path.display());
}

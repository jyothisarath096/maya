fn main() {
    println!("cargo:rerun-if-changed=src/arch/boot.s");
    println!("cargo:rerun-if-changed=aarch64.ld");

    let out = std::env::var("OUT_DIR").expect("OUT_DIR not set");

    std::process::Command::new("aarch64-elf-as")
        .args(["src/arch/boot.s", "-o", &format!("{out}/boot.o")])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .status()
        .expect("failed to assemble boot.s");

    std::process::Command::new("aarch64-elf-ar")
        .args([
            "crs",
            &format!("{out}/libboot.a"),
            &format!("{out}/boot.o"),
        ])
        .status()
        .expect("failed to archive");

    println!("cargo:rustc-link-search={out}");
    println!("cargo:rustc-link-lib=static=boot");
}

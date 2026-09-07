extern crate cbindgen;

use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let config_path = PathBuf::from(&crate_dir).join("cbindgen.toml");
    let out_dir = PathBuf::from(&crate_dir).join("include");

    std::fs::create_dir_all(&out_dir).unwrap();

    let config = cbindgen::Config::from_file(&config_path).unwrap_or_default();

    cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .expect("Unable to generate C bindings with cbindgen")
        .write_to_file(out_dir.join("wpffi.h"));

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}

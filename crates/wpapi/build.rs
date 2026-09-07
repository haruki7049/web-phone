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
        .write_to_file(out_dir.join("wpapi.h"));

    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".to_string());
    let description = env::var("CARGO_PKG_DESCRIPTION").unwrap_or_else(|_| {
        "Core client engine and C API bindings for web-phone WebRTC audio system".to_string()
    });

    let pc_content = format!(
        "prefix=/usr/local\n\
         exec_prefix=${{prefix}}\n\
         libdir=${{exec_prefix}}/lib\n\
         includedir=${{prefix}}/include\n\
         \n\
         Name: wpapi\n\
         Description: {}\n\
         Version: {}\n\
         Libs: -L${{libdir}} -lwpapi\n\
         Cflags: -I${{includedir}}\n",
        description, version
    );

    std::fs::write(out_dir.join("wpapi.pc"), pc_content).expect("Unable to write wpapi.pc");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-changed=Cargo.toml");
}

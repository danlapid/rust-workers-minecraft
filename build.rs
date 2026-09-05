use std::{env, path::PathBuf};

fn main() {
    println!("cargo::rerun-if-changed=worker/fs-library.js");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("emscripten") {
        return;
    }
    // Keep growth headroom bounded after generation's temporary allocations.
    println!("cargo::rustc-link-arg-bins=-sMEMORY_GROWTH_LINEAR_STEP=2097152");
    let library =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("worker/fs-library.js");
    println!(
        "cargo::rustc-link-arg-bins=-sDEFAULT_LIBRARY_FUNCS_TO_INCLUDE=[\"$workerFilesystem\"]"
    );
    println!("cargo::rustc-link-arg-bins=--js-library");
    println!("cargo::rustc-link-arg-bins={}", library.display());
}

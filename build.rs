use std::{env, path::PathBuf};

// worker-build supplies the common emscripten link settings; these are the
// application's own.
fn main() {
    println!("cargo::rerun-if-changed=src/workerd.js");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("emscripten") {
        return;
    }
    for arg in [
        "-sNODERAWFS",
        "-sSTACK_SIZE=8MB",
        // Keep growth headroom bounded after generation's temporary allocations.
        "-sMEMORY_GROWTH_LINEAR_STEP=2097152",
        "-sDEFAULT_LIBRARY_FUNCS_TO_INCLUDE=[\"$workerdFs\"]",
        "--js-library",
    ] {
        println!("cargo::rustc-link-arg-bins={arg}");
    }
    let library = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("src/workerd.js");
    println!("cargo::rustc-link-arg-bins={}", library.display());
}

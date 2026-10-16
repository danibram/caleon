//! A placeholder `web/dist` so `cargo build` works before `pnpm build` in `web/`.

use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../web/dist");
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist");
    if !dist.exists() {
        fs::create_dir_all(&dist).expect("create web/dist");
        fs::write(
            dist.join("index.html"),
            "<!doctype html><html><head><meta charset=\"utf-8\"><title>°CALEON</title></head><body><p>Run <code>pnpm build</code> in <code>web/</code>.</p></body></html>",
        )
        .expect("write placeholder index.html");
    }
}

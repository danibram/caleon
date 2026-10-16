//! Writes the TypeScript types of the web app to web/src/api/generated: `cargo test export_types`.

use caleon::app::Snapshot;
use caleon::connection::Settings;
use std::path::PathBuf;
use ts_rs::TS;

#[test]
fn export_types() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web/src/api/generated");
    std::fs::create_dir_all(&out).expect("create generated dir");
    Snapshot::export_all_to(&out).expect("export Snapshot");
    Settings::export_all_to(&out).expect("export Settings");
}

//! Writes the TypeScript types of the web app to web/src/api/generated: `cargo test export_types`.

use caleon_server::app::Snapshot;
use caleon_server::connection::Settings;
use caleon_server::weather::runtime::WeatherView;
use caleon_server::weather::{
    ProviderCapabilities, ProviderErrorKind, SelectionPeriod, StationCandidate, WeatherPoint,
    WeatherSettings,
};
use std::path::PathBuf;
use ts_rs::TS;

#[test]
fn export_types() {
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../web/src/api/generated");
    std::fs::create_dir_all(&out).expect("create generated dir");
    Snapshot::export_all_to(&out).expect("export Snapshot");
    Settings::export_all_to(&out).expect("export Settings");
    WeatherSettings::export_all_to(&out).expect("export WeatherSettings");
    WeatherPoint::export_all_to(&out).expect("export WeatherPoint");
    SelectionPeriod::export_all_to(&out).expect("export SelectionPeriod");
    ProviderCapabilities::export_all_to(&out).expect("export ProviderCapabilities");
    ProviderErrorKind::export_all_to(&out).expect("export ProviderErrorKind");
    StationCandidate::export_all_to(&out).expect("export StationCandidate");
    WeatherView::export_all_to(&out).expect("export WeatherView");
}

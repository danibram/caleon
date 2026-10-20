use caleon_server::store::{RoomSample, Store};
use caleon_server::weather::{
    ApiKey, Coordinates, Location, ProviderId, SampleKind, Selection, SelectionTarget, Station,
    TimeRange, WeatherConfig, WeatherSample, WeatherSource,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TestDir(PathBuf);

impl TestDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "caleon-weather-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn home() -> Coordinates {
    Coordinates {
        latitude: 40.4,
        longitude: -3.7,
    }
}

fn station() -> Station {
    Station {
        id: "3195".into(),
        name: "Estación de prueba".into(),
        coordinates: Coordinates {
            latitude: 40.45,
            longitude: -3.55,
        },
        elevation_m: Some(600.0),
    }
}

fn selection(aemet: bool) -> Selection {
    Selection {
        location: Location {
            name: "Vivienda de prueba".into(),
            coordinates: home(),
        },
        target: if aemet {
            SelectionTarget::Aemet { station: station() }
        } else {
            SelectionTarget::OpenMeteo
        },
    }
}

fn source(aemet: bool) -> WeatherSource {
    WeatherSource {
        provider: if aemet {
            ProviderId::Aemet
        } else {
            ProviderId::OpenMeteo
        },
        product: if aemet { "observation" } else { "forecast" }.into(),
        kind: if aemet {
            SampleKind::Observation
        } else {
            SampleKind::Estimate
        },
        requested_coordinates: if aemet { station().coordinates } else { home() },
        coordinates: if aemet { station().coordinates } else { home() },
        station_id: aemet.then(|| station().id),
        resolution_seconds: 3600,
        attribution: if aemet { "AEMET" } else { "Open-Meteo" }.into(),
    }
}

fn sample(ts: i64, temperature_c: f64, fetched_at: i64) -> WeatherSample {
    WeatherSample {
        ts,
        temperature_c,
        fetched_at,
    }
}

fn range(start: i64, end: i64) -> TimeRange {
    TimeRange { start, end }
}

fn indoor(temperature: Option<f64>) -> RoomSample {
    RoomSample {
        room: 0,
        temperature,
        humidity: Some(50.0),
        tset: None,
        mode: "off".into(),
    }
}

#[test]
fn configuration_gates_collection_and_keeps_secrets_out_of_public_settings() {
    let dir = TestDir::new();
    let mut config = WeatherConfig::load(&dir.0).unwrap();
    assert!(!config.can_collect(true));
    config.enabled = true;
    assert!(config.validate().is_err());
    config.selection = Some(selection(false));
    assert!(config.can_collect(true));
    assert!(!config.can_collect(false));

    config.selection = Some(selection(true));
    assert!(config.validate().is_err());
    config.aemet_api_key = Some(ApiKey::new("unit-test-secret").unwrap());
    assert!(config.can_collect(true));
    config.save(&dir.0).unwrap();
    let loaded = WeatherConfig::load(&dir.0).unwrap();
    let public = serde_json::to_string(&loaded.settings()).unwrap();
    assert!(!public.contains("unit-test-secret"));
    assert!(!public.contains("aemet_api_key"));
    assert!(loaded.settings().aemet_key_configured);
    assert!(!format!("{loaded:?}").contains("unit-test-secret"));
    assert_eq!(
        loaded.aemet_api_key.as_ref().unwrap().expose_secret(),
        "unit-test-secret"
    );

    config.enabled = false;
    config.save(&dir.0).unwrap();
    let paused = WeatherConfig::load(&dir.0).unwrap();
    assert!(!paused.can_collect(true));
    assert_eq!(paused.selection, config.selection);
    assert!(paused.settings().aemet_key_configured);
    assert!(!dir.0.join("settings.json").exists());
    assert!(dir.0.join("history.sqlite").exists());
    assert!(!dir.0.join("weather.json").exists());
}

#[test]
fn invalid_save_preserves_the_last_configuration_and_corruption_is_reported() {
    let dir = TestDir::new();
    let mut config = WeatherConfig {
        enabled: true,
        selection: Some(selection(false)),
        ..Default::default()
    };
    config.save(&dir.0).unwrap();
    config.selection = Some(selection(true));
    assert!(config.save(&dir.0).is_err());
    assert_eq!(
        WeatherConfig::load(&dir.0).unwrap().selection,
        Some(selection(false))
    );
    let db = rusqlite::Connection::open(dir.0.join("history.sqlite")).unwrap();
    db.execute("UPDATE weather_config SET selection = '{broken'", [])
        .unwrap();
    assert!(WeatherConfig::load(&dir.0).is_err());
}

#[cfg(unix)]
#[test]
fn generated_master_key_is_private_and_not_created_without_credentials() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TestDir::new();
    WeatherConfig::default().save(&dir.0).unwrap();
    let file = dir.0.join("secrets.key");
    assert!(!file.exists());
    WeatherConfig {
        aemet_api_key: Some(ApiKey::new("private-test-secret").unwrap()),
        ..Default::default()
    }
    .save(&dir.0)
    .unwrap();
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(std::fs::read(file).unwrap().len(), 32);
}

#[test]
fn sqlite_configuration_encrypts_credentials_and_preserves_history() {
    let dir = TestDir::new();
    let path = dir.0.join("history.sqlite");
    let store = Store::open(&path).unwrap();
    store.add_rooms(100, &[indoor(Some(20.0))]).unwrap();
    let config = WeatherConfig {
        enabled: true,
        selection: Some(selection(true)),
        selection_since: Some(42),
        aemet_api_key: Some(ApiKey::new("encrypted-test-secret").unwrap()),
    };
    config.save(&dir.0).unwrap();
    assert_eq!(WeatherConfig::load(&dir.0).unwrap(), config);
    drop(store);
    let db = rusqlite::Connection::open(&path).unwrap();
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM samples", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
    let cipher: Vec<u8> = db
        .query_row("SELECT ciphertext FROM credentials", [], |r| r.get(0))
        .unwrap();
    assert!(!cipher.windows(21).any(|w| w == b"encrypted-test-secret"));
    db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
    drop(db);
    let bytes = std::fs::read(path).unwrap();
    assert!(!bytes.windows(21).any(|w| w == b"encrypted-test-secret"));
}

#[test]
fn missing_or_wrong_master_key_blocks_loading_and_saving_without_erasing_data() {
    let dir = TestDir::new();
    let config = WeatherConfig {
        aemet_api_key: Some(ApiKey::new("recoverable-secret").unwrap()),
        ..Default::default()
    };
    config.save(&dir.0).unwrap();
    let key_path = dir.0.join("secrets.key");
    let original = std::fs::read(&key_path).unwrap();
    std::fs::remove_file(&key_path).unwrap();
    assert!(WeatherConfig::load(&dir.0).is_err());
    assert!(WeatherConfig::default().save(&dir.0).is_err());
    assert!(!key_path.exists());
    for wrong in [vec![0u8; 32], vec![0u8; 12]] {
        std::fs::write(&key_path, wrong).unwrap();
        assert!(WeatherConfig::load(&dir.0).is_err());
        assert!(WeatherConfig::default().save(&dir.0).is_err());
    }
    std::fs::write(key_path, original).unwrap();
    assert_eq!(WeatherConfig::load(&dir.0).unwrap(), config);
}

#[test]
fn damaged_ciphertext_is_rejected_and_preserved() {
    let dir = TestDir::new();
    WeatherConfig {
        aemet_api_key: Some(ApiKey::new("tamper-test-secret").unwrap()),
        ..Default::default()
    }
    .save(&dir.0)
    .unwrap();
    let db = rusqlite::Connection::open(dir.0.join("history.sqlite")).unwrap();
    db.execute("UPDATE credentials SET ciphertext = zeroblob(40)", [])
        .unwrap();
    assert!(WeatherConfig::load(&dir.0).is_err());
    assert!(WeatherConfig::default().save(&dir.0).is_err());
    let bytes: Vec<u8> = db
        .query_row("SELECT ciphertext FROM credentials", [], |r| r.get(0))
        .unwrap();
    assert_eq!(bytes, vec![0u8; 40]);
}

#[test]
fn opening_weather_storage_preserves_existing_indoor_history() {
    let dir = TestDir::new();
    let path = dir.0.join("history.sqlite");
    {
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE samples (ts INTEGER NOT NULL, room INTEGER NOT NULL,
            temperature REAL, humidity REAL, tset REAL, mode TEXT, PRIMARY KEY(ts, room));
            INSERT INTO samples VALUES (100, 0, 21.5, 50, 22, 'regular');",
        )
        .unwrap();
    }
    for _ in 0..2 {
        let store = Store::open(&path).unwrap();
        assert_eq!(store.room_history(0).unwrap()["0"][0][1], 21.5);
        assert!(store
            .weather_history(range(0, 200), false)
            .unwrap()
            .is_empty());
    }
}

#[test]
fn recovered_observations_win_without_deleting_estimates_after_reopening() {
    let dir = TestDir::new();
    let path = dir.0.join("history.sqlite");
    let period;
    {
        let store = Store::open(&path).unwrap();
        period = store.select_weather_source(&selection(true), 0).unwrap();
        store
            .add_weather_samples(period, &source(false), &[sample(100, 10.0, 200)])
            .unwrap();
        assert_eq!(
            store.weather_history(range(0, 300), true).unwrap()[0]
                .source
                .provider,
            ProviderId::OpenMeteo
        );
        store
            .add_weather_samples(period, &source(true), &[sample(100, 9.0, 210)])
            .unwrap();
        store
            .add_weather_samples(period, &source(false), &[sample(100, 11.0, 220)])
            .unwrap();
    }
    let store = Store::open(&path).unwrap();
    let preferred = store.weather_history(range(0, 300), true).unwrap();
    assert_eq!(preferred.len(), 1);
    assert_eq!(preferred[0].period_id, period);
    assert_eq!(preferred[0].source.provider, ProviderId::Aemet);
    assert_eq!(preferred[0].sample.temperature_c, 9.0);
    let all = store.weather_history(range(0, 300), false).unwrap();
    assert_eq!(all.len(), 2);
    assert!(all
        .iter()
        .any(|p| p.source.provider == ProviderId::OpenMeteo && p.sample.temperature_c == 11.0));
    assert_ne!(all[0].source_id, all[1].source_id);
}

#[test]
fn deduplication_ignores_old_downloads_and_accepts_new_revisions() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let id = store.select_weather_source(&selection(false), 0).unwrap();
    for (temp, fetched) in [(10.0, 200), (10.0, 200), (8.0, 150), (11.0, 220)] {
        store
            .add_weather_samples(id, &source(false), &[sample(100, temp, fetched)])
            .unwrap();
    }
    let points = store.weather_history(range(100, 101), false).unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].sample, sample(100, 11.0, 220));
    assert!(store
        .weather_history(range(0, 100), false)
        .unwrap()
        .is_empty());
}

#[test]
fn changing_station_preserves_periods_and_rejects_misattributed_readings() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let first = selection(true);
    let mut second = first.clone();
    if let SelectionTarget::Aemet { station } = &mut second.target {
        station.id = "new-station".into();
    }
    let old_id = store.select_weather_source(&first, 0).unwrap();
    assert_eq!(store.select_weather_source(&first, 1).unwrap(), old_id);
    store
        .add_weather_samples(old_id, &source(true), &[sample(100, 9.0, 150)])
        .unwrap();
    let new_id = store.select_weather_source(&second, 200).unwrap();
    assert_ne!(new_id, old_id);
    assert!(store
        .add_weather_samples(new_id, &source(true), &[sample(200, 12.0, 220)])
        .is_err());
    assert!(store
        .add_weather_samples(old_id, &source(true), &[sample(200, 12.0, 220)])
        .is_err());
    let mut new_source = source(true);
    new_source.station_id = Some("new-station".into());
    store
        .add_weather_samples(new_id, &new_source, &[sample(200, 12.0, 220)])
        .unwrap();
    let periods = store.weather_selections(range(50, 300)).unwrap();
    assert_eq!(periods.len(), 2);
    assert_eq!(periods[0].range, range(50, 200));
    assert_eq!(periods[1].range, range(200, 300));
    let points = store.weather_history(range(0, 300), true).unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!(points[0].source.station_id, Some(station().id));
    assert_eq!(points[1].source.station_id.as_deref(), Some("new-station"));
}

#[test]
fn provider_and_home_changes_keep_the_previous_history() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let aemet_id = store.select_weather_source(&selection(true), 0).unwrap();
    store
        .add_weather_samples(aemet_id, &source(true), &[sample(100, 9.0, 110)])
        .unwrap();
    let model_id = store.select_weather_source(&selection(false), 200).unwrap();
    assert!(store
        .add_weather_samples(model_id, &source(true), &[sample(200, 9.0, 210)])
        .is_err());
    store
        .add_weather_samples(model_id, &source(false), &[sample(200, 10.0, 210)])
        .unwrap();
    let mut moved = selection(false);
    moved.location.coordinates.longitude = -3.8;
    let moved_id = store.select_weather_source(&moved, 300).unwrap();
    assert!(store
        .add_weather_samples(moved_id, &source(false), &[sample(300, 10.0, 310)])
        .is_err());
    assert_eq!(
        store.weather_history(range(0, 400), false).unwrap().len(),
        2
    );
    assert!(store.select_weather_source(&selection(true), 250).is_err());
    assert_eq!(store.weather_selections(range(0, 400)).unwrap().len(), 3);
}

#[test]
fn failed_batch_is_atomic_and_retroactive_changes_do_not_hide_existing_samples() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let id = store.select_weather_source(&selection(false), 100).unwrap();
    store
        .add_weather_samples(id, &source(false), &[sample(200, 10.0, 220)])
        .unwrap();
    assert!(store
        .add_weather_samples(
            id,
            &source(false),
            &[sample(250, 11.0, 300), sample(50, 12.0, 300)]
        )
        .is_err());
    assert!(store.select_weather_source(&selection(true), 150).is_err());
    let periods = store.weather_selections(range(0, 400)).unwrap();
    assert_eq!(periods.len(), 1);
    assert_eq!(periods[0].selection, selection(false));
    assert_eq!(
        store.weather_history(range(0, 400), false).unwrap().len(),
        1
    );
}

#[test]
fn initial_history_can_precede_activation_and_same_second_edits_leave_no_empty_ranges() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    let first = store.select_weather_source(&selection(true), 100).unwrap();
    let second = store.select_weather_source(&selection(false), 100).unwrap();
    assert_ne!(first, second);
    let periods = store.weather_selections(range(0, 300)).unwrap();
    assert_eq!(periods.len(), 1);
    assert_eq!(periods[0].id, second);
    store
        .add_weather_samples(second, &source(false), &[sample(100, 10.0, 300)])
        .unwrap();
    assert_eq!(
        store.weather_history(range(0, 300), false).unwrap().len(),
        1
    );
}

#[test]
fn import_ranges_only_cover_buckets_with_indoor_temperatures() {
    let store = Store::open(std::path::Path::new(":memory:")).unwrap();
    assert!(store
        .indoor_temperature_ranges(range(0, 15000), 3600)
        .unwrap()
        .is_empty());
    store.add_rooms(100, &[indoor(None)]).unwrap();
    assert!(store
        .indoor_temperature_ranges(range(0, 15000), 3600)
        .unwrap()
        .is_empty());
    for ts in [3700, 3800, 7300, 14500] {
        store.add_rooms(ts, &[indoor(Some(21.0))]).unwrap();
    }
    assert_eq!(
        store
            .indoor_temperature_ranges(range(0, 15000), 3600)
            .unwrap(),
        vec![range(3600, 10800), range(14400, 15000)]
    );
    assert_eq!(
        store
            .indoor_temperature_ranges(range(3750, 7500), 3600)
            .unwrap(),
        vec![range(3750, 7500)]
    );
    assert!(store.indoor_temperature_ranges(range(0, 100), 0).is_err());
}

#[test]
fn malformed_locations_sources_and_samples_are_rejected() {
    for coordinates in [
        Coordinates {
            latitude: f64::NAN,
            longitude: 0.0,
        },
        Coordinates {
            latitude: 91.0,
            longitude: 0.0,
        },
        Coordinates {
            latitude: 0.0,
            longitude: f64::INFINITY,
        },
    ] {
        assert!(coordinates.validate().is_err());
    }
    let mut invalid = source(false);
    invalid.kind = SampleKind::Observation;
    assert!(invalid.validate().is_err());
    invalid = source(true);
    invalid.station_id = None;
    assert!(invalid.validate().is_err());
    invalid = source(false);
    invalid.resolution_seconds = 0;
    assert!(invalid.validate().is_err());
    assert!(sample(100, f64::NAN, 200).validate().is_err());
    assert!(sample(100, 12.0, 99).validate().is_err());
    assert!(range(100, 100).validate().is_err());
    assert!(ApiKey::new("  ").is_err());
}

#[test]
fn failed_configuration_save_rolls_back_credentials() {
    let dir = TestDir::new();
    WeatherConfig::load(&dir.0).unwrap();
    let db = rusqlite::Connection::open(dir.0.join("history.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_weather_config BEFORE INSERT ON weather_config
        BEGIN SELECT RAISE(ABORT, 'test write failure'); END;",
    )
    .unwrap();
    let config = WeatherConfig {
        aemet_api_key: Some(ApiKey::new("rollback-secret").unwrap()),
        ..Default::default()
    };
    assert!(config.save(&dir.0).is_err());
    assert_eq!(
        WeatherConfig::load(&dir.0).unwrap(),
        WeatherConfig::default()
    );
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM credentials", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    db.execute_batch("DROP TRIGGER reject_weather_config")
        .unwrap();
    config.save(&dir.0).unwrap();
    assert_eq!(WeatherConfig::load(&dir.0).unwrap(), config);
}

#[test]
fn configuration_saves_preserve_every_selection_period_without_waiting_for_collection() {
    let dir = TestDir::new();
    let mut config = WeatherConfig {
        enabled: true,
        selection: Some(selection(false)),
        selection_since: Some(100),
        ..Default::default()
    };
    config.save(&dir.0).unwrap();
    let first = config.selection.clone().unwrap();
    config.selection.as_mut().unwrap().location.name = "Second selection".into();
    config.selection_since = Some(200);
    config.save(&dir.0).unwrap();
    let second = config.selection.clone().unwrap();
    config.selection.as_mut().unwrap().location.name = "Third selection".into();
    config.selection_since = Some(300);
    config.save(&dir.0).unwrap();
    let store = Store::open(&dir.0.join("history.sqlite")).unwrap();
    let periods = store.weather_selections(range(100, 400)).unwrap();
    assert_eq!(periods.len(), 3);
    assert_eq!(periods[0].selection, first);
    assert_eq!(periods[0].range, range(100, 200));
    assert_eq!(periods[1].selection, second);
    assert_eq!(periods[1].range, range(200, 300));
    assert_eq!(periods[2].selection, config.selection.unwrap());
    assert_eq!(periods[2].range, range(300, 400));
}

#[test]
fn failed_selection_change_rolls_back_configuration_and_credentials_together() {
    let dir = TestDir::new();
    let original = WeatherConfig {
        enabled: true,
        selection: Some(selection(false)),
        selection_since: Some(100),
        ..Default::default()
    };
    original.save(&dir.0).unwrap();
    let db = rusqlite::Connection::open(dir.0.join("history.sqlite")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_weather_selection BEFORE INSERT ON weather_selections
        BEGIN SELECT RAISE(ABORT, 'test selection failure'); END;",
    )
    .unwrap();
    let changed = WeatherConfig {
        enabled: true,
        selection: Some(selection(true)),
        selection_since: Some(200),
        aemet_api_key: Some(ApiKey::new("rollback-selection-secret").unwrap()),
    };
    assert!(changed.save(&dir.0).is_err());
    assert_eq!(WeatherConfig::load(&dir.0).unwrap(), original);
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM credentials", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    let periods = Store::open(&dir.0.join("history.sqlite"))
        .unwrap()
        .weather_selections(range(0, 400))
        .unwrap();
    assert_eq!(periods.len(), 1);
    assert_eq!(periods[0].selection, original.selection.unwrap());
    assert_eq!(periods[0].range, range(100, 400));
}

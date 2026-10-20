use super::{
    aemet::Aemet, http, open_meteo::OpenMeteo, ApiKey, ProviderErrorKind, ProviderId, Selection,
    SelectionTarget, TimeRange, WeatherConfig, WeatherPoint, WeatherProvider, WeatherSettings,
};
use crate::app::{now, App};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, RwLock,
    },
};
use tokio::sync::{Mutex, Notify};
use ts_rs::TS;

const MONTH: i64 = 30 * 86400;

type ProviderFactory =
    dyn Fn(&Selection, &WeatherConfig) -> Result<Box<dyn WeatherProvider>> + Send + Sync;

#[derive(Debug, Clone, Default, Serialize, TS)]
pub struct WeatherStatus {
    pub effective_provider: Option<ProviderId>,
    #[ts(type = "number | null")]
    pub last_sample_at: Option<i64>,
    #[ts(type = "number | null")]
    pub last_success_at: Option<i64>,
    #[ts(type = "number | null")]
    pub backup_since: Option<i64>,
    pub error: Option<String>,
    pub recovering: bool,
    pub recovery: Option<RecoveryReport>,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
pub struct RecoveryReport {
    pub recovered_hours: usize,
    pub remaining_hours: usize,
    pub failed_queries: usize,
}

#[derive(Debug, Serialize, TS)]
pub struct WeatherView {
    pub settings: WeatherSettings,
    pub status: WeatherStatus,
    pub history_enabled: bool,
    pub aemet_api_key_url: String,
}

#[derive(Deserialize)]
pub struct WeatherUpdate {
    pub enabled: bool,
    pub selection: Option<Selection>,
    pub aemet_api_key: Option<String>,
}

pub struct WeatherRuntime {
    pub(crate) client: reqwest::Client,
    providers: Box<ProviderFactory>,
    config: RwLock<WeatherConfig>,
    status: RwLock<WeatherStatus>,
    pub history_enabled: bool,
    revision: AtomicU64,
    update: Mutex<()>,
    operation: Mutex<()>,
    wake: Notify,
    recover: AtomicBool,
}

impl WeatherRuntime {
    pub fn new(dir: &Path, history_enabled: bool) -> Result<Self> {
        let (config, error) = match WeatherConfig::load(dir) {
            Ok(config) => (config, None),
            Err(_) => (
                WeatherConfig::default(),
                Some("No se pudo leer la configuración meteorológica. Comprueba SQLite y la clave maestra de credenciales antes de guardar cambios.".into()),
            ),
        };
        let client = http::client()?;
        let provider_client = client.clone();
        let providers = Box::new(
            move |selection: &Selection,
                  config: &WeatherConfig|
                  -> Result<Box<dyn WeatherProvider>> {
                Ok(match selection.target {
                    SelectionTarget::OpenMeteo => Box::new(OpenMeteo::new(provider_client.clone())),
                    SelectionTarget::Aemet { .. } => Box::new(Aemet::new(
                        provider_client.clone(),
                        config
                            .aemet_api_key
                            .clone()
                            .ok_or_else(|| anyhow::anyhow!("Introduce una clave AEMET"))?,
                    )),
                })
            },
        );
        Ok(Self {
            client,
            providers,
            config: RwLock::new(config),
            status: RwLock::new(WeatherStatus {
                error,
                ..Default::default()
            }),
            history_enabled,
            revision: AtomicU64::new(0),
            update: Mutex::new(()),
            operation: Mutex::new(()),
            wake: Notify::new(),
            recover: AtomicBool::new(true),
        })
    }

    pub fn view(&self) -> WeatherView {
        WeatherView {
            settings: self.config.read().unwrap().settings(),
            status: self.status.read().unwrap().clone(),
            history_enabled: self.history_enabled,
            aemet_api_key_url: super::aemet::API_KEY_URL.into(),
        }
    }

    pub fn configured_key(&self, supplied: Option<String>) -> Result<ApiKey> {
        if let Some(value) = supplied.filter(|s| !s.trim().is_empty()) {
            return ApiKey::new(&value);
        }
        self.config
            .read()
            .unwrap()
            .aemet_api_key
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Introduce una clave AEMET"))
    }

    fn provider(
        &self,
        selection: &Selection,
        config: &WeatherConfig,
    ) -> Result<Box<dyn WeatherProvider>> {
        (self.providers)(selection, config)
    }

    pub async fn save(&self, app: &Arc<App>, input: WeatherUpdate) -> Result<WeatherView> {
        let _update = self.update.lock().await;
        let mut config = self.config.read().unwrap().clone();
        let previous = config.selection.clone();
        config.enabled = input.enabled;
        if input.enabled {
            config.selection = input.selection;
            // a key only replaces the saved one for an AEMET target, where the station check below
            // proves it works
            let aemet = matches!(
                config.selection.as_ref().map(|s| &s.target),
                Some(SelectionTarget::Aemet { .. })
            );
            if let Some(key) = input
                .aemet_api_key
                .filter(|key| aemet && !key.trim().is_empty())
            {
                config.aemet_api_key = Some(ApiKey::new(&key)?);
            }
            config.validate()?;
            let provider = self.provider(config.selection.as_ref().unwrap(), &config)?;
            let selection = config.selection.as_mut().unwrap();
            match &selection.target {
                SelectionTarget::Aemet { station } => {
                    let candidates = provider.stations(selection.location.coordinates).await?;
                    let actual = candidates
                        .into_iter()
                        .find(|candidate| candidate.station.id == station.id)
                        .ok_or_else(|| {
                            anyhow::anyhow!("La estación no tiene temperatura reciente disponible")
                        })?;
                    selection.target = SelectionTarget::Aemet {
                        station: actual.station,
                    };
                }
                SelectionTarget::OpenMeteo => {
                    let batches = provider
                        .temperatures(
                            selection,
                            TimeRange {
                                start: now() - 10800,
                                end: now() + 1,
                            },
                        )
                        .await?;
                    ensure!(
                        batches.iter().any(|batch| !batch.samples.is_empty()),
                        "No hay temperatura reciente para esta ubicación"
                    );
                }
            }
            if config.selection != previous {
                config.selection_since = Some(
                    blocking(app, |app| {
                        let existing = app.store.weather_selections(TimeRange {
                            start: 0,
                            end: now() + 1,
                        })?;
                        Ok(if existing.is_empty() {
                            app.store
                                .indoor_temperature_ranges(
                                    TimeRange {
                                        start: (now() - MONTH).max(0),
                                        end: now() + 1,
                                    },
                                    3600,
                                )?
                                .first()
                                .map_or(now() / 3600 * 3600, |range| range.start)
                        } else {
                            now() + 1
                        })
                    })
                    .await?,
                );
            }
        }
        config.validate()?;
        // saves are serialised by `update`, so the file can be written before taking the lock that
        // readers of the configuration wait on
        let saved = config.clone();
        blocking(app, move |app| saved.save(&app.data_dir)).await?;
        {
            let mut current = self.config.write().unwrap();
            *current = config;
            self.revision.fetch_add(1, Ordering::SeqCst);
            *self.status.write().unwrap() = WeatherStatus::default();
            self.recover.store(input.enabled, Ordering::SeqCst);
        }
        self.wake.notify_one();
        Ok(self.view())
    }

    pub fn request_recovery(&self) -> Result<()> {
        let config = self.config.read().unwrap();
        ensure!(
            config.can_collect(self.history_enabled),
            "La recogida meteorológica está desactivada"
        );
        ensure!(
            !self.status.read().unwrap().recovering,
            "Ya hay una recuperación en curso"
        );
        // `recovering` turns on when the worker actually starts it, so an early return in `tick`
        // cannot leave it stuck; asking again while one is pending is the same request
        self.recover.store(true, Ordering::SeqCst);
        self.wake.notify_one();
        Ok(())
    }

    fn current(&self, revision: u64) -> bool {
        self.revision.load(Ordering::SeqCst) == revision
    }

    async fn recent(
        &self,
        app: &App,
        config: &WeatherConfig,
        revision: u64,
        period_id: i64,
        range: TimeRange,
    ) -> Result<()> {
        let selection = config.selection.as_ref().unwrap();
        let provider = self.provider(selection, config)?;
        let mut result = provider.temperatures(selection, range).await;
        if !self.current(revision) {
            return Ok(());
        }
        if result
            .as_ref()
            .is_err_and(|e| e.kind == ProviderErrorKind::Unavailable)
        {
            result = provider.temperatures(selection, range).await;
        }
        let latest = result
            .as_ref()
            .ok()
            .and_then(|b| b.iter().flat_map(|b| &b.samples).map(|s| s.ts).max());
        let mut backup_since = None;
        // a period that began less than two hours ago may simply not have its first hourly sample yet
        let young = now() - range.start < 7200;
        let waiting = young && result.as_ref().is_ok() && latest.is_none();
        let fallback = selection.provider() == ProviderId::Aemet
            && !waiting
            && (result.is_err() || latest.is_none_or(|ts| now() - ts > 7200));
        let mut error = result.as_ref().err().map(ToString::to_string);
        if fallback {
            if !self.current(revision) {
                return Ok(());
            }
            error = Some(error.unwrap_or_else(|| "AEMET no tiene una temperatura reciente".into()));
            // the backup starts right after the last AEMET hour, so the hours in between are filled too
            let since = {
                let status = self.status.read().unwrap();
                status.backup_since.unwrap_or_else(|| {
                    status
                        .last_sample_at
                        .map_or(range.start, |ts| (ts / 3600 + 1) * 3600)
                })
            };
            backup_since = Some(since);
            let backup_selection = Selection {
                location: selection.location.clone(),
                target: SelectionTarget::OpenMeteo,
            };
            let backup = self
                .provider(&backup_selection, config)?
                .temperatures(
                    &backup_selection,
                    TimeRange {
                        start: range.start.max(since),
                        end: range.end,
                    },
                )
                .await;
            if let Ok(batches) = result {
                self.persist(app, config, revision, period_id, batches)?;
            }
            result = backup;
        }
        let batches = result.inspect_err(|e| {
            if self.current(revision) {
                self.status.write().unwrap().error = Some(if fallback {
                    format!(
                        "{}. Open-Meteo tampoco ha podido obtener datos",
                        error.as_deref().unwrap_or("AEMET no está disponible")
                    )
                } else {
                    e.to_string()
                });
            }
        })?;
        let latest = batches.iter().flat_map(|b| &b.samples).map(|s| s.ts).max();
        let guard = self.config.read().unwrap();
        if !self.current(revision) || !guard.can_collect(self.history_enabled) {
            return Ok(());
        }
        let Some(latest) = latest else {
            if waiting {
                self.status.write().unwrap().error = None;
                return Ok(());
            }
            let message = if fallback {
                format!(
                    "{}. Open-Meteo no tiene temperaturas disponibles para este periodo",
                    error.as_deref().unwrap_or("AEMET no está disponible")
                )
            } else {
                "El proveedor no tiene temperaturas disponibles para este periodo".into()
            };
            self.status.write().unwrap().error = Some(message.clone());
            anyhow::bail!(message);
        };
        for batch in batches {
            app.store
                .add_weather_samples(period_id, &batch.source, &batch.samples)?;
        }
        let mut status = self.status.write().unwrap();
        status.effective_provider = Some(if fallback {
            ProviderId::OpenMeteo
        } else {
            selection.provider()
        });
        status.last_sample_at = Some(
            status
                .last_sample_at
                .map_or(latest, |previous| previous.max(latest)),
        );
        status.last_success_at = Some(now());
        status.error = error;
        status.backup_since = if fallback {
            status.backup_since.or(backup_since)
        } else {
            None
        };
        Ok(())
    }

    fn persist(
        &self,
        app: &App,
        _config: &WeatherConfig,
        revision: u64,
        period_id: i64,
        batches: Vec<super::WeatherBatch>,
    ) -> Result<()> {
        let guard = self.config.read().unwrap();
        if self.current(revision) && guard.can_collect(self.history_enabled) {
            for batch in batches {
                app.store
                    .add_weather_samples(period_id, &batch.source, &batch.samples)?;
            }
        }
        Ok(())
    }

    async fn recover_history(
        &self,
        app: &App,
        config: &WeatherConfig,
        revision: u64,
    ) -> Result<RecoveryReport> {
        let range = TimeRange {
            start: (now() - MONTH).max(0),
            end: now() + 1,
        };
        let indoor = app.store.indoor_temperature_ranges(range, 3600)?;
        let before = app.store.weather_history(range, true)?;
        let mut report = RecoveryReport::default();
        if indoor.is_empty() {
            return Ok(report);
        }
        for period in app.store.weather_selections(range)? {
            if !self.current(revision) {
                break;
            }
            let eligible: Vec<_> = indoor
                .iter()
                .filter_map(|r| {
                    let start = r.start.max(period.range.start);
                    let end = r.end.min(period.range.end);
                    (start < end).then_some(TimeRange { start, end })
                })
                .collect();
            let (Some(first), Some(last)) = (eligible.first(), eligible.last()) else {
                continue;
            };
            match self
                .provider(&period.selection, config)?
                .temperatures(
                    &period.selection,
                    TimeRange {
                        start: first.start,
                        end: last.end,
                    },
                )
                .await
            {
                Ok(mut batches) => {
                    for batch in &mut batches {
                        batch.samples.retain(|sample| {
                            eligible
                                .iter()
                                .any(|r| sample.ts >= r.start && sample.ts < r.end)
                        });
                    }
                    self.persist(app, config, revision, period.id, batches)?;
                }
                Err(_) => report.failed_queries += 1,
            }
            // AEMET only serves the last hours: older gaps in an AEMET period come from Open-Meteo at
            // the home's location, stored with their own source so they read as estimates
            if period.selection.provider() == ProviderId::Aemet && self.current(revision) {
                let have: std::collections::HashSet<i64> = app
                    .store
                    .weather_history(range, true)?
                    .iter()
                    .filter(|p| p.period_id == period.id)
                    .map(|p| p.sample.ts / 3600)
                    .collect();
                let missing: std::collections::BTreeSet<i64> = eligible
                    .iter()
                    .flat_map(|r| (r.start / 3600)..=((r.end - 1) / 3600))
                    .filter(|hour| !have.contains(hour))
                    .collect();
                if let (Some(&first), Some(&last)) = (missing.first(), missing.last()) {
                    let backup = Selection {
                        location: period.selection.location.clone(),
                        target: SelectionTarget::OpenMeteo,
                    };
                    match self
                        .provider(&backup, config)?
                        .temperatures(
                            &backup,
                            TimeRange {
                                start: first * 3600,
                                end: (last + 1) * 3600,
                            },
                        )
                        .await
                    {
                        Ok(mut batches) => {
                            for batch in &mut batches {
                                batch.samples.retain(|sample| {
                                    missing.contains(&(sample.ts / 3600))
                                        && eligible
                                            .iter()
                                            .any(|r| sample.ts >= r.start && sample.ts < r.end)
                                });
                            }
                            self.persist(app, config, revision, period.id, batches)?;
                        }
                        Err(_) => report.failed_queries += 1,
                    }
                }
            }
        }
        let after = app.store.weather_history(range, true)?;
        let covered = |points: &[WeatherPoint], ts: i64| {
            points.iter().any(|p| p.sample.ts / 3600 == ts / 3600)
        };
        for period in indoor {
            let mut ts = period.start;
            while ts < period.end {
                if covered(&after, ts) {
                    if !covered(&before, ts) {
                        report.recovered_hours += 1;
                    }
                } else {
                    report.remaining_hours += 1;
                }
                ts = (ts / 3600 + 1) * 3600;
            }
        }
        Ok(report)
    }

    fn begin_recovery(&self, revision: u64) -> bool {
        let config = self.config.read().unwrap();
        if !self.current(revision)
            || !config.can_collect(self.history_enabled)
            || !self.recover.swap(false, Ordering::SeqCst)
        {
            return false;
        }
        self.status.write().unwrap().recovering = true;
        true
    }

    async fn tick(&self, app: &Arc<App>) -> Result<()> {
        let _operation = self.operation.lock().await;
        let (config, revision) = {
            let guard = self.config.read().unwrap();
            if !guard.can_collect(self.history_enabled) {
                return Ok(());
            }
            (guard.clone(), self.revision.load(Ordering::SeqCst))
        };
        let since = config.selection_since.unwrap_or(now() / 3600 * 3600);
        if since > now() {
            return Ok(());
        }
        let selection = config.selection.clone().unwrap();
        let period_id = blocking(app, move |app| {
            app.store.select_weather_source(&selection, since)
        })
        .await?;
        if self.status.read().unwrap().last_sample_at.is_none() {
            let last = blocking(app, move |app| {
                Ok(app
                    .store
                    .weather_history(
                        TimeRange {
                            start: (now() - MONTH).max(0),
                            end: now() + 1,
                        },
                        true,
                    )?
                    .into_iter()
                    .rev()
                    .find(|point| point.period_id == period_id)
                    .map(|point| point.sample.ts))
            })
            .await?;
            if let Some(ts) = last {
                self.status.write().unwrap().last_sample_at = Some(ts);
            }
        }
        let recent_range = TimeRange {
            start: (now() - 43200).max(since),
            end: now() + 1,
        };
        let recent = self
            .recent(app, &config, revision, period_id, recent_range)
            .await;
        if !self.current(revision) {
            return Ok(());
        }
        if self.begin_recovery(revision) {
            let result = self.recover_history(app, &config, revision).await;
            if self.current(revision) {
                let mut status = self.status.write().unwrap();
                status.recovering = false;
                match result {
                    Ok(report) => status.recovery = Some(report),
                    Err(_) => status.error = Some("No se pudo completar la recuperación".into()),
                };
            }
        }
        recent
    }
}

/// Runs SQLite work off the async runtime.
async fn blocking<T: Send + 'static>(
    app: &Arc<App>,
    f: impl FnOnce(&App) -> Result<T> + Send + 'static,
) -> Result<T> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || f(&app)).await?
}

pub fn spawn(app: Arc<App>) {
    tokio::spawn(async move {
        loop {
            if app.weather.tick(&app).await.is_err() {
                tracing::debug!("weather collection did not complete");
            }
            let pending = app
                .weather
                .config
                .read()
                .unwrap()
                .selection_since
                .is_some_and(|ts| ts > now());
            tokio::select! {_=tokio::time::sleep(std::time::Duration::from_secs(if pending {1} else {900}))=>{},_=app.weather.wake.notified()=>{}}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weather::{
        Coordinates, Location, ProviderCapabilities, ProviderError, ProviderFuture, SampleKind,
        Station, WeatherBatch, WeatherSample, WeatherSource,
    };
    use crate::{
        app::Snapshot,
        connection::Settings,
        store::{RoomSample, Store},
    };
    use caleon::{
        device::Device,
        schema::Arg,
        transport::{Found, Transport},
    };
    use std::{
        collections::VecDeque,
        path::PathBuf,
        sync::{atomic::AtomicUsize, Mutex as StdMutex},
    };

    type Replies = Arc<StdMutex<VecDeque<std::result::Result<Vec<WeatherBatch>, ProviderError>>>>;
    #[derive(Clone)]
    struct FakeProvider {
        id: ProviderId,
        replies: Replies,
        calls: Arc<StdMutex<Vec<ProviderId>>>,
        pause: Option<(Arc<Notify>, Arc<Notify>)>,
    }
    impl WeatherProvider for FakeProvider {
        fn id(&self) -> ProviderId {
            self.id
        }
        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                stations: self.id == ProviderId::Aemet,
                requires_api_key: self.id == ProviderId::Aemet,
                historical_hourly: self.id == ProviderId::OpenMeteo,
            }
        }
        fn temperatures<'a>(
            &'a self,
            _selection: &'a Selection,
            _range: TimeRange,
        ) -> ProviderFuture<'a, Vec<WeatherBatch>> {
            Box::pin(async move {
                self.calls.lock().unwrap().push(self.id);
                if let Some((started, release)) = &self.pause {
                    started.notify_one();
                    release.notified().await;
                }
                self.replies
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected weather request")
            })
        }
    }
    struct NoThermostat;
    impl Transport for NoThermostat {
        fn kind(&self) -> &'static str {
            "test"
        }
        fn invoke(
            &self,
            _query: &str,
            _args: &[(&str, Arg)],
            _retry: bool,
        ) -> Result<serde_json::Value> {
            panic!("weather must not call the thermostat")
        }
    }
    struct Fixture {
        app: Arc<App>,
        dir: PathBuf,
        calls: Arc<StdMutex<Vec<ProviderId>>>,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.dir).unwrap();
        }
    }

    fn config(aemet: bool) -> WeatherConfig {
        let home = Coordinates {
            latitude: 40.4,
            longitude: -3.7,
        };
        WeatherConfig {
            enabled: true,
            selection: Some(Selection {
                location: Location {
                    name: "Casa".into(),
                    coordinates: home,
                },
                target: if aemet {
                    SelectionTarget::Aemet {
                        station: Station {
                            id: "test-station".into(),
                            name: "Estación".into(),
                            coordinates: home,
                            elevation_m: Some(600.0),
                        },
                    }
                } else {
                    SelectionTarget::OpenMeteo
                },
            }),
            aemet_api_key: Some(ApiKey::new("test-only-key").unwrap()),
            selection_since: Some(now() - 86400),
        }
    }
    fn batches(aemet: bool, ts: i64) -> Vec<WeatherBatch> {
        let location = Coordinates {
            latitude: 40.4,
            longitude: -3.7,
        };
        vec![WeatherBatch {
            source: WeatherSource {
                provider: if aemet {
                    ProviderId::Aemet
                } else {
                    ProviderId::OpenMeteo
                },
                product: if aemet { "observation" } else { "model" }.into(),
                kind: if aemet {
                    SampleKind::Observation
                } else {
                    SampleKind::Estimate
                },
                requested_coordinates: location,
                coordinates: location,
                station_id: aemet.then(|| "test-station".into()),
                resolution_seconds: 3600,
                attribution: "Test fixture".into(),
            },
            samples: vec![WeatherSample {
                ts,
                temperature_c: if aemet { 10.0 } else { 11.0 },
                fetched_at: now(),
            }],
        }]
    }
    fn failure(kind: ProviderErrorKind) -> std::result::Result<Vec<WeatherBatch>, ProviderError> {
        Err(http::error(kind, "Provider fixture failure"))
    }

    fn fixture(
        config: WeatherConfig,
        history: bool,
        primary: Vec<std::result::Result<Vec<WeatherBatch>, ProviderError>>,
        backup: Vec<std::result::Result<Vec<WeatherBatch>, ProviderError>>,
        pause: Option<(Arc<Notify>, Arc<Notify>)>,
    ) -> Fixture {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "caleon-runtime-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        let mut weather = WeatherRuntime::new(&dir, history).unwrap();
        *weather.config.write().unwrap() = config;
        let calls = Arc::new(StdMutex::new(Vec::new()));
        let aemet = FakeProvider {
            id: ProviderId::Aemet,
            replies: Arc::new(StdMutex::new(primary.into())),
            calls: calls.clone(),
            pause,
        };
        let open_meteo = FakeProvider {
            id: ProviderId::OpenMeteo,
            replies: Arc::new(StdMutex::new(backup.into())),
            calls: calls.clone(),
            pause: None,
        };
        weather.providers = Box::new(move |selection, _| {
            Ok(Box::new(if selection.provider() == ProviderId::Aemet {
                aemet.clone()
            } else {
                open_meteo.clone()
            }))
        });
        let app = Arc::new(App {
            device: Device::new("test".into(), Arc::new(NoThermostat)),
            store: Store::open(&dir.join("history.sqlite")).unwrap(),
            weather,
            data_dir: dir.clone(),
            snapshot: RwLock::new(Snapshot::default()),
            crawling: AtomicBool::new(false),
            crawl_done: AtomicUsize::new(0),
            crawl_expected: AtomicUsize::new(0),
            backup_hours: 0,
            read_only: true,
            prometheus: false,
            web: false,
            schema: Arc::new(caleon::schema().unwrap()),
            conn: RwLock::new((
                Settings {
                    transport: "udp".into(),
                    host: "127.0.0.1".into(),
                    email: String::new(),
                },
                Found {
                    id: "test".into(),
                    addr: "127.0.0.1:5570".parse().unwrap(),
                },
            )),
        });
        Fixture { app, dir, calls }
    }

    #[tokio::test]
    async fn off_and_history_disabled_never_request_a_provider() {
        let mut disabled = config(true);
        disabled.enabled = false;
        for (config, history) in [(disabled, true), (config(true), false)] {
            let f = fixture(config, history, vec![], vec![], None);
            f.app.weather.tick(&f.app).await.unwrap();
            assert!(f.calls.lock().unwrap().is_empty());
            assert!(f.app.weather.request_recovery().is_err());
        }
    }

    #[tokio::test]
    async fn retry_then_backup_and_recovery_preserve_both_origins() {
        let ts = now() / 3600 * 3600;
        let f = fixture(
            config(true),
            true,
            vec![
                failure(ProviderErrorKind::Unavailable),
                failure(ProviderErrorKind::Unavailable),
                Ok(batches(true, ts)),
            ],
            vec![Ok(batches(false, ts))],
            None,
        );
        let config = f.app.weather.config.read().unwrap().clone();
        let id = f
            .app
            .store
            .select_weather_source(config.selection.as_ref().unwrap(), ts - 86400)
            .unwrap();
        let range = TimeRange {
            start: ts - 3600,
            end: now() + 1,
        };
        f.app
            .weather
            .recent(&f.app, &config, 0, id, range)
            .await
            .unwrap();
        assert_eq!(
            f.app.weather.view().status.effective_provider,
            Some(ProviderId::OpenMeteo)
        );
        assert!(f.app.weather.view().status.backup_since.is_some());
        f.app
            .weather
            .recent(&f.app, &config, 0, id, range)
            .await
            .unwrap();
        assert_eq!(
            f.app.weather.view().status.effective_provider,
            Some(ProviderId::Aemet)
        );
        assert!(f.app.weather.view().status.backup_since.is_none());
        assert_eq!(f.app.store.weather_history(range, false).unwrap().len(), 2);
        assert_eq!(
            f.app.store.weather_history(range, true).unwrap()[0]
                .source
                .kind,
            SampleKind::Observation
        );
        assert_eq!(
            *f.calls.lock().unwrap(),
            vec![
                ProviderId::Aemet,
                ProviderId::Aemet,
                ProviderId::OpenMeteo,
                ProviderId::Aemet
            ]
        );
    }

    #[tokio::test]
    async fn rejected_key_uses_backup_without_retrying_the_key() {
        let ts = now() / 3600 * 3600;
        let f = fixture(
            config(true),
            true,
            vec![failure(ProviderErrorKind::Authentication)],
            vec![Ok(batches(false, ts))],
            None,
        );
        let config = f.app.weather.config.read().unwrap().clone();
        let id = f
            .app
            .store
            .select_weather_source(config.selection.as_ref().unwrap(), ts - 86400)
            .unwrap();
        f.app
            .weather
            .recent(
                &f.app,
                &config,
                0,
                id,
                TimeRange {
                    start: ts - 3600,
                    end: now() + 1,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            *f.calls.lock().unwrap(),
            vec![ProviderId::Aemet, ProviderId::OpenMeteo]
        );
        assert!(f.app.weather.view().status.error.is_some());
    }

    #[tokio::test]
    async fn stale_observation_activates_backup_even_when_http_succeeds() {
        let ts = now() / 3600 * 3600;
        let f = fixture(
            config(true),
            true,
            vec![Ok(batches(true, ts - 10800))],
            vec![Ok(batches(false, ts))],
            None,
        );
        let config = f.app.weather.config.read().unwrap().clone();
        let id = f
            .app
            .store
            .select_weather_source(config.selection.as_ref().unwrap(), ts - 86400)
            .unwrap();
        f.app
            .weather
            .recent(
                &f.app,
                &config,
                0,
                id,
                TimeRange {
                    start: ts - 14400,
                    end: now() + 1,
                },
            )
            .await
            .unwrap();
        assert_eq!(
            f.app.weather.view().status.effective_provider,
            Some(ProviderId::OpenMeteo)
        );
        assert_eq!(
            f.app
                .store
                .weather_history(
                    TimeRange {
                        start: ts - 14400,
                        end: now() + 1
                    },
                    false
                )
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn disabling_during_request_prevents_retry_backup_and_persistence() {
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let f = fixture(
            config(true),
            true,
            vec![failure(ProviderErrorKind::Unavailable)],
            vec![],
            Some((started.clone(), release.clone())),
        );
        let app = f.app.clone();
        let task = tokio::spawn(async move { app.weather.tick(&app).await });
        started.notified().await;
        let off = f
            .app
            .weather
            .save(
                &f.app,
                WeatherUpdate {
                    enabled: false,
                    selection: None,
                    aemet_api_key: None,
                },
            )
            .await
            .unwrap();
        assert!(!off.settings.enabled);
        release.notify_one();
        task.await.unwrap().unwrap();
        assert_eq!(*f.calls.lock().unwrap(), vec![ProviderId::Aemet]);
        assert!(f
            .app
            .store
            .weather_history(
                TimeRange {
                    start: 0,
                    end: now() + 1
                },
                false
            )
            .unwrap()
            .is_empty());
        assert!(off.settings.aemet_key_configured);
    }

    #[tokio::test]
    async fn recovery_without_indoor_temperatures_makes_no_requests() {
        let f = fixture(config(false), true, vec![], vec![], None);
        let config = f.app.weather.config.read().unwrap().clone();
        let report = f
            .app
            .weather
            .recover_history(&f.app, &config, 0)
            .await
            .unwrap();
        assert_eq!(report.recovered_hours, 0);
        assert!(f.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn recovery_excludes_indoor_samples_older_than_thirty_days() {
        let ts = now() / 3600 * 3600;
        let old = ts - 40 * 86400;
        let recent = ts - 20 * 86400;
        let mut config = config(false);
        config.selection_since = Some(old);
        let mut batch = batches(false, recent);
        batch[0].samples.push(WeatherSample {
            ts: old,
            temperature_c: 5.0,
            fetched_at: now(),
        });
        let f = fixture(config.clone(), true, vec![], vec![Ok(batch)], None);
        f.app
            .store
            .select_weather_source(config.selection.as_ref().unwrap(), old)
            .unwrap();
        for ts in [old + 60, recent + 60] {
            f.app
                .store
                .add_rooms(
                    ts,
                    &[RoomSample {
                        room: 0,
                        temperature: Some(21.0),
                        humidity: None,
                        tset: None,
                        mode: "off".into(),
                    }],
                )
                .unwrap();
        }
        let report = f
            .app
            .weather
            .recover_history(&f.app, &config, 0)
            .await
            .unwrap();
        assert_eq!(report.recovered_hours, 1);
        let points = f
            .app
            .store
            .weather_history(
                TimeRange {
                    start: 0,
                    end: now() + 1,
                },
                false,
            )
            .unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].sample.ts, recent);
    }
    #[tokio::test]
    async fn recovery_fills_old_aemet_gaps_from_open_meteo() {
        let ts = now() / 3600 * 3600;
        let old = ts - 2 * 86400;
        let mut config = config(true);
        config.selection_since = Some(old - 3600);
        // AEMET answers with the last hour only; Open-Meteo has the old one
        let f = fixture(
            config.clone(),
            true,
            vec![Ok(batches(true, ts - 3600))],
            vec![Ok(batches(false, old))],
            None,
        );
        f.app
            .store
            .select_weather_source(config.selection.as_ref().unwrap(), old - 3600)
            .unwrap();
        for at in [old + 60, ts - 3600 + 60] {
            f.app
                .store
                .add_rooms(
                    at,
                    &[RoomSample {
                        room: 0,
                        temperature: Some(21.0),
                        humidity: None,
                        tset: None,
                        mode: "off".into(),
                    }],
                )
                .unwrap();
        }
        let report = f
            .app
            .weather
            .recover_history(&f.app, &config, 0)
            .await
            .unwrap();
        assert_eq!(report.recovered_hours, 2);
        assert_eq!(report.remaining_hours, 0);
        let points = f
            .app
            .store
            .weather_history(
                TimeRange {
                    start: 0,
                    end: now() + 1,
                },
                true,
            )
            .unwrap();
        assert!(points
            .iter()
            .any(|p| p.sample.ts == old && p.source.provider == ProviderId::OpenMeteo));
        assert!(points
            .iter()
            .any(|p| p.sample.ts == ts - 3600 && p.source.provider == ProviderId::Aemet));
    }

    #[tokio::test]
    async fn a_period_that_just_began_waits_for_its_first_hourly_sample() {
        let mut config = config(true);
        config.selection_since = Some(now() - 600);
        let f = fixture(config, true, vec![Ok(vec![])], vec![], None);
        f.app.weather.tick(&f.app).await.unwrap();
        let status = f.app.weather.view().status;
        assert!(status.error.is_none());
        assert!(status.backup_since.is_none());
        assert_eq!(status.effective_provider, None);
    }

    #[tokio::test]
    async fn both_providers_down_preserve_history_and_last_reading_age() {
        let ts = now() / 3600 * 3600 - 7200;
        let f = fixture(
            config(true),
            true,
            vec![
                failure(ProviderErrorKind::Unavailable),
                failure(ProviderErrorKind::Unavailable),
            ],
            vec![failure(ProviderErrorKind::Unavailable)],
            None,
        );
        let config = f.app.weather.config.read().unwrap().clone();
        let id = f
            .app
            .store
            .select_weather_source(
                config.selection.as_ref().unwrap(),
                config.selection_since.unwrap(),
            )
            .unwrap();
        let batch = batches(false, ts).pop().unwrap();
        f.app
            .store
            .add_weather_samples(id, &batch.source, &batch.samples)
            .unwrap();
        assert!(f.app.weather.tick(&f.app).await.is_err());
        let status = f.app.weather.view().status;
        assert_eq!(status.last_sample_at, Some(ts));
        assert!(status.error.unwrap().contains("Open-Meteo"));
        assert_eq!(
            f.app
                .store
                .weather_history(
                    TimeRange {
                        start: 0,
                        end: now() + 1
                    },
                    false
                )
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn empty_primary_or_backup_preserves_the_last_reading_without_claiming_success() {
        let ts = now() / 3600 * 3600 - 7200;
        for aemet in [false, true] {
            let f = fixture(
                config(aemet),
                true,
                if aemet {
                    vec![failure(ProviderErrorKind::Authentication)]
                } else {
                    vec![]
                },
                vec![Ok(vec![])],
                None,
            );
            let config = f.app.weather.config.read().unwrap().clone();
            let id = f
                .app
                .store
                .select_weather_source(
                    config.selection.as_ref().unwrap(),
                    config.selection_since.unwrap(),
                )
                .unwrap();
            let batch = batches(false, ts).pop().unwrap();
            f.app
                .store
                .add_weather_samples(id, &batch.source, &batch.samples)
                .unwrap();
            assert!(f.app.weather.tick(&f.app).await.is_err());
            let status = f.app.weather.view().status;
            assert_eq!(status.last_sample_at, Some(ts));
            assert_eq!(status.last_success_at, None);
            assert_eq!(status.effective_provider, None);
            assert!(status.error.unwrap().contains("temperaturas disponibles"));
            assert_eq!(
                f.app
                    .store
                    .weather_history(
                        TimeRange {
                            start: ts,
                            end: now() + 1
                        },
                        false
                    )
                    .unwrap()
                    .len(),
                1
            );
        }
    }

    #[test]
    fn a_recovery_request_is_pending_until_the_worker_starts_it() {
        let f = fixture(config(false), true, vec![], vec![], None);
        f.app.weather.request_recovery().unwrap();
        assert!(!f.app.weather.view().status.recovering);
        f.app.weather.request_recovery().unwrap();
        assert!(f.app.weather.begin_recovery(0));
        assert!(f.app.weather.view().status.recovering);
    }

    #[test]
    fn obsolete_worker_does_not_consume_recovery_requested_for_a_new_configuration() {
        let f = fixture(config(false), true, vec![], vec![], None);
        f.app.weather.revision.store(1, Ordering::SeqCst);
        f.app.weather.recover.store(true, Ordering::SeqCst);
        assert!(!f.app.weather.begin_recovery(0));
        assert!(!f.app.weather.view().status.recovering);
        assert!(f.app.weather.begin_recovery(1));
        assert!(f.app.weather.view().status.recovering);
        assert!(!f.app.weather.begin_recovery(1));
    }

    #[tokio::test]
    async fn rapid_selection_changes_are_persisted_while_an_old_request_is_pending() {
        let started = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let ts = now() / 3600 * 3600;
        let f = fixture(
            config(true),
            true,
            vec![Ok(batches(true, ts))],
            vec![Ok(batches(false, ts)), Ok(batches(false, ts))],
            Some((started.clone(), release.clone())),
        );
        f.app.weather.config.read().unwrap().save(&f.dir).unwrap();
        let app = f.app.clone();
        let task = tokio::spawn(async move { app.weather.tick(&app).await });
        started.notified().await;
        let mut second = config(false).selection.unwrap();
        second.location.name = "Second selection".into();
        let mut third = second.clone();
        third.location.name = "Third selection".into();
        for selection in [second, third.clone()] {
            let view = f
                .app
                .weather
                .save(
                    &f.app,
                    WeatherUpdate {
                        enabled: true,
                        selection: Some(selection.clone()),
                        aemet_api_key: None,
                    },
                )
                .await
                .unwrap();
            assert_eq!(view.settings.selection, Some(selection.clone()));
            let saved = WeatherConfig::load(&f.dir).unwrap();
            assert_eq!(saved.selection, Some(selection.clone()));
            let periods = f
                .app
                .store
                .weather_selections(TimeRange {
                    start: 0,
                    end: now() + 5,
                })
                .unwrap();
            assert_eq!(periods.last().unwrap().selection, selection);
        }
        release.notify_one();
        task.await.unwrap().unwrap();
        assert_eq!(f.app.weather.view().settings.selection, Some(third));
        assert!(f
            .app
            .store
            .weather_history(
                TimeRange {
                    start: 0,
                    end: now() + 1
                },
                false
            )
            .unwrap()
            .is_empty());
        let db = rusqlite::Connection::open(f.dir.join("history.sqlite")).unwrap();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM weather_selections", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }
}

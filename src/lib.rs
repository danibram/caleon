pub mod app;
pub mod connection;
pub mod device;
#[cfg(feature = "nabto-sdk")]
pub mod nabto;
pub mod schema;
pub mod status;
pub mod store;
pub mod transport;
pub mod web;

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use std::time::Duration;

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

pub async fn run() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let data_dir = PathBuf::from(env("DATA_DIR", "./data"));
    let bind = env("BIND", "0.0.0.0:8088");
    let poll: u64 = env("POLL_SECONDS", "60").parse().context("POLL_SECONDS")?;
    let backup_hours: u64 = env("BACKUP_HOURS", "24").parse().context("BACKUP_HOURS")?;
    let backup_keep: usize = env("BACKUP_KEEP", "30").parse().context("BACKUP_KEEP")?;
    std::fs::create_dir_all(&data_dir)?;

    let settings = connection::Settings::load(&data_dir);
    let want_id = env("CALEON_DEVICE", "");
    let found = {
        let (host, want) = (settings.host.clone(), want_id.clone());
        tokio::task::spawn_blocking(move || connection::locate(&host, &want)).await??
    };
    let device_id = found.id.clone();
    tracing::info!("°CALEON {device_id} at {}", found.addr);
    let schema = Arc::new(schema::Schema::parse(connection::INTERFACE_XML)?);
    let transport = connection::build(&settings, &found, &schema, &data_dir)?;
    tracing::info!("connection mode: {}", transport.kind());

    let app = Arc::new(app::App {
        device: device::Device::new(device_id.clone(), transport),
        store: store::Store::open(&data_dir.join("history.sqlite"))?,
        data_dir: data_dir.clone(),
        snapshot: RwLock::new(app::Snapshot {
            device: device_id.clone(),
            ..Default::default()
        }),
        crawling: AtomicBool::new(false),
        crawl_done: std::sync::atomic::AtomicUsize::new(0),
        crawl_expected: std::sync::atomic::AtomicUsize::new(0),
        backup_hours,
        read_only: matches!(env("READ_ONLY", "0").as_str(), "1" | "true" | "yes"),
        schema,
        conn: RwLock::new((settings, found)),
    });
    if let Some(name) = app
        .list_backups()?
        .first()
        .and_then(|b| b["name"].as_str().map(String::from))
    {
        if let Ok(b) = app.read_backup(&name) {
            let mut snap = app.snapshot.write().unwrap();
            if let Some(z) = b["zones"].as_object() {
                snap.zones = z.clone();
            }
            if let Some(x) = b["sensors"].as_object() {
                snap.sensors = x.clone();
            }
        }
    }

    {
        let app = app.clone();
        tokio::spawn(async move {
            loop {
                let a = app.clone();
                let r = tokio::task::spawn_blocking(move || a.refresh(true)).await;
                if let Ok(Err(e)) | Err(e) = r.map_err(anyhow::Error::from) {
                    tracing::warn!("refresh failed: {e:#}");
                    app.snapshot.write().unwrap().error = Some(format!("{e:#}"));
                }
                tokio::time::sleep(Duration::from_secs(poll)).await;
            }
        });
    }

    if backup_hours > 0 {
        let app = app.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(20)).await;
            loop {
                let a = app.clone();
                match tokio::task::spawn_blocking(move || {
                    let n = a.create_backup("automática", true);
                    a.prune_backups(backup_keep);
                    n
                })
                .await
                {
                    Ok(Ok(Some(name))) => tracing::info!("automatic backup {name}"),
                    Ok(Ok(None)) => {}
                    Ok(Err(e)) => tracing::warn!("automatic backup failed: {e:#}"),
                    Err(e) => tracing::warn!("automatic backup panicked: {e}"),
                }
                tokio::time::sleep(Duration::from_secs(backup_hours * 3600)).await;
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    if app.read_only {
        tracing::warn!("READ_ONLY: writes to the thermostat are disabled");
    }
    tracing::info!("web UI on http://{bind}");
    axum::serve(listener, web::router(app))
        .with_graceful_shutdown(async {
            let mut term =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler");
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = term.recv() => {}
            }
        })
        .await?;
    std::process::exit(0);
}

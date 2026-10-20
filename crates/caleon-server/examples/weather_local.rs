//! Runs real weather routes with indoor fixtures and no connection to a thermostat.
use anyhow::{bail, Result};
use caleon::{
    device::Device,
    schema::Arg,
    transport::{Found, Transport},
};
use caleon_server::{
    app::{now, App, Snapshot},
    connection::Settings,
    store::{RoomSample, Store},
    weather::runtime::WeatherRuntime,
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize},
        Arc, RwLock,
    },
};

struct NoDevice;
impl Transport for NoDevice {
    fn kind(&self) -> &'static str {
        "fixture"
    }
    fn invoke(&self, _query: &str, _args: &[(&str, Arg)], _retry: bool) -> Result<Value> {
        bail!("Local weather validation has no thermostat transport")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let dir = std::env::var("DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("caleon-weather-local"));
    std::fs::create_dir_all(&dir)?;
    let history = std::env::var("HISTORY").as_deref() != Ok("0");
    let store = Store::open(&dir.join("history.sqlite"))?;
    let state: Value = serde_json::from_str(include_str!("../../../web/src/mocks/state.json"))?;
    let samples: Value = serde_json::from_str(include_str!("../../../web/src/mocks/history.json"))?;
    let shift = now() - state["updated"].as_i64().unwrap();
    for (room, rows) in samples.as_object().unwrap() {
        for row in rows.as_array().unwrap() {
            store.add_rooms(
                row[0].as_i64().unwrap() + shift,
                &[RoomSample {
                    room: room.parse()?,
                    temperature: row[1].as_f64(),
                    humidity: row[2].as_f64(),
                    tset: row[3].as_f64(),
                    mode: row[4].as_str().unwrap_or("").into(),
                }],
            )?;
        }
    }
    let app = Arc::new(App {
        device: Device::new("local-weather-fixture".into(), Arc::new(NoDevice)),
        store,
        weather: WeatherRuntime::new(&dir, history)?,
        data_dir: dir,
        snapshot: RwLock::new(Snapshot::default()),
        crawling: AtomicBool::new(false),
        crawl_done: AtomicUsize::new(0),
        crawl_expected: AtomicUsize::new(0),
        backup_hours: 0,
        read_only: true,
        prometheus: false,
        web: false,
        schema: Arc::new(caleon::schema()?),
        conn: RwLock::new((
            Settings {
                transport: "udp".into(),
                host: "127.0.0.1".into(),
                email: String::new(),
            },
            Found {
                id: "local-weather-fixture".into(),
                addr: "127.0.0.1:5570".parse()?,
            },
        )),
    });
    caleon_server::weather::runtime::spawn(app.clone());
    let bind = std::env::var("BIND").unwrap_or_else(|_| "127.0.0.1:18089".into());
    println!("Weather validation server on http://{bind}");
    axum::serve(
        tokio::net::TcpListener::bind(bind).await?,
        caleon_server::web::router(app),
    )
    .await?;
    Ok(())
}

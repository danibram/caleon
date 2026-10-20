//! HTTP API and the embedded web UI.

use crate::app::{now, parse_number, App};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use caleon::device::{self, RoomChange, ROOM_MODES, SYSTEM_ROOM};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

type AppState = Arc<App>;

pub struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError(StatusCode::BAD_GATEWAY, e.to_string())
    }
}

/// Weather errors: a provider failure is a bad gateway, a storage failure is ours, anything else
/// is a message about the request.
fn weather_error(e: anyhow::Error) -> ApiError {
    let status = if e.is::<crate::weather::ProviderError>() {
        StatusCode::BAD_GATEWAY
    } else if e.is::<rusqlite::Error>()
        || e.is::<std::io::Error>()
        || e.is::<tokio::task::JoinError>()
    {
        StatusCode::INTERNAL_SERVER_ERROR
    } else {
        StatusCode::BAD_REQUEST
    };
    ApiError(status, e.to_string())
}

fn guard_write(app: &App) -> Result<(), ApiError> {
    if app.read_only {
        Err(ApiError(
            StatusCode::FORBIDDEN,
            "Modo solo lectura: este servidor no puede cambiar el termostato".into(),
        ))
    } else {
        Ok(())
    }
}

fn mode_label(m: &str) -> &str {
    match m {
        "off" => "Apagado",
        "regular" => "Normal",
        "eco" => "Eco",
        "turbo" => "Confort",
        other => other,
    }
}

fn fmt_temp(v: f64) -> String {
    format!("{v:.1} °C").replace('.', ",")
}

fn timer_kind(id: u16, t: u8) -> &'static str {
    if id == SYSTEM_ROOM {
        " (agua caliente)"
    } else if t == 1 {
        " (refrigeración)"
    } else {
        ""
    }
}

/// Menu actions that only move between screens (allowed in read-only mode).
fn is_navigation(action: &str) -> bool {
    action.is_empty()
        || action.starts_with("goto:")
        || matches!(action, "back" | "forward" | "exit" | "status")
}

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

type ApiResult = Result<Json<Value>, ApiError>;

/// Bumped by every write, so a newer write takes over the rereads of the previous one.
static REREAD_GEN: AtomicU64 = AtomicU64::new(0);

/// After a change, reread everything a few more times in the background, because valves, pump and
/// the other rooms react seconds later. Writes in a row (a week of schedule, several taps on +)
/// share one series of rereads. The poller keeps its own one-minute rhythm.
/// With `room`, only that room is reread (four calls instead of ten), right away and ten seconds
/// later, when the valves have reacted. Without it, everything is reread at 5 and 15 s.
pub(crate) fn reread_later(app: &AppState, room: Option<u16>, now: bool) {
    let gen = REREAD_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tokio::spawn(async move {
        let waits: &[u64] = if room.is_some() {
            &[0, 10]
        } else if now {
            &[0, 15]
        } else {
            &[5, 15]
        };
        for secs in waits {
            tokio::time::sleep(std::time::Duration::from_secs(*secs)).await;
            if REREAD_GEN.load(Ordering::SeqCst) != gen {
                return;
            }
            let a = app.clone();
            let r = tokio::task::spawn_blocking(move || match room {
                Some(id) => a.refresh_room(id),
                None => a.refresh(false),
            })
            .await;
            if let Ok(Err(e)) = r {
                tracing::debug!("reread after change: {e}");
            }
        }
    });
}

/// Runs blocking device work off the async runtime.
async fn blocking<T: Send + 'static>(
    app: &AppState,
    f: impl FnOnce(&App) -> anyhow::Result<T> + Send + 'static,
) -> Result<T, ApiError> {
    let app = app.clone();
    tokio::task::spawn_blocking(move || f(&app))
        .await
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(Into::into)
}

pub fn router(app: AppState) -> Router {
    let api = Router::new()
        .route("/icons/:name", get(icon))
        .route("/metrics", get(prometheus))
        .route("/api/state", get(state))
        .route("/api/refresh", post(refresh))
        .route("/api/history", get(history))
        .route("/api/metrics", get(metrics))
        .route("/api/log", get(message_log))
        .route("/api/events", get(events))
        .route("/api/rooms/:id", post(room_change))
        .route("/api/rooms/:id/modes", post(room_modes))
        .route("/api/rooms/:id/timer", get(timer_get))
        .route("/api/rooms/:id/timer/:day", put(timer_put))
        .route("/api/rooms/:id/timer/:day/copy", post(timer_copy))
        .route("/api/menu/action", post(menu_action))
        .route("/api/menu/detail", get(menu_detail))
        .route("/api/menu/set", post(menu_set))
        .route("/api/menu/open", post(menu_open))
        .route("/api/menu/values", get(menu_values))
        .route("/api/settings", get(settings_get).post(settings_set))
        .route("/api/weather", get(weather_get).post(weather_set))
        .route("/api/weather/locations", get(weather_locations))
        .route("/api/weather/stations", post(weather_stations))
        .route("/api/weather/history", get(weather_history))
        .route("/api/weather/recover", post(weather_recover))
        .route("/api/backups", get(backups_list).post(backup_create))
        .route("/api/backups/:name", get(backup_get).delete(backup_delete))
        .route("/api/backups/:name/restore", post(backup_restore));
    let router = if app.web {
        api.route("/", get(index)).fallback(get(static_file))
    } else {
        api.fallback(get(no_web))
    };
    router.with_state(app)
}

async fn no_web() -> Response {
    (
        StatusCode::NOT_FOUND,
        "web UI disabled (WEB=0); the API is under /api and Prometheus metrics under /metrics",
    )
        .into_response()
}

async fn index() -> impl IntoResponse {
    spa_index()
}

#[derive(rust_embed::RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../web/dist"]
struct WebDist;

#[derive(rust_embed::RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/../../static/icons"]
struct Icons;

fn spa_index() -> Response {
    match WebDist::get("index.html") {
        Some(file) => (
            [
                (header::CACHE_CONTROL, "no-cache"),
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            ],
            file.data.into_owned(),
        )
            .into_response(),
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Web UI not built: run pnpm build in web/",
        )
            .into_response(),
    }
}

fn asset_mime(path: &str) -> &'static str {
    if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else if path.ends_with(".woff2") {
        "font/woff2"
    } else {
        "application/octet-stream"
    }
}

/// Files of the built web app (`/assets/index-*.js`, `/favicon.svg`...). Anything else that is
/// not an API path gets the app's index, so a bookmarked page still loads.
async fn static_file(uri: axum::http::Uri) -> Response {
    let key = uri.path().trim_start_matches('/');
    match WebDist::get(key) {
        Some(file) => (
            [
                (header::CONTENT_TYPE, asset_mime(key)),
                (
                    header::CACHE_CONTROL,
                    if key.starts_with("assets/") {
                        "public, max-age=31536000, immutable"
                    } else {
                        "no-cache"
                    },
                ),
            ],
            file.data.into_owned(),
        )
            .into_response(),
        None if key.starts_with("api/") || key.starts_with("assets/") => {
            StatusCode::NOT_FOUND.into_response()
        }
        None => spa_index(),
    }
}

/// The app's SVG icons style themselves through classes; give them colours here.
async fn icon(Path(name): Path<String>, Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(file) = Icons::get(&name) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let svg = String::from_utf8_lossy(&file.data);
    // ?mask=1 keeps only the artwork, for CSS masks coloured by the page theme
    let style = if q.contains_key("mask") {
        "<style>.icon-background{fill:none}.icon-foreground{fill:#000}</style>"
    } else {
        "<style>.icon-background{fill:#f2612a}.icon-foreground{fill:#fff}</style>"
    };
    let svg = match svg
        .find("<svg")
        .and_then(|i| svg[i..].find('>').map(|j| i + j + 1))
    {
        Some(at) => format!("{}{style}{}", &svg[..at], &svg[at..]),
        None => svg.into_owned(),
    };
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "max-age=86400"),
        ],
        svg,
    )
        .into_response()
}

async fn state(State(app): State<AppState>) -> Json<Value> {
    let (calls, last_minute) = app.device.calls();
    let mut v = serde_json::to_value(&*app.snapshot.read().unwrap()).unwrap();
    v["device_calls"] = json!({"total": calls, "last_minute": last_minute});
    v["crawling"] = json!(app.crawling.load(Ordering::SeqCst));
    v["crawl_done"] = json!(app.crawl_done.load(Ordering::SeqCst));
    v["crawl_expected"] = json!(app.crawl_expected.load(Ordering::SeqCst));
    v["history_since"] = json!(app.store.history_since());
    v["backup_hours"] = json!(app.backup_hours);
    v["read_only"] = json!(app.read_only);
    v["mode_temps"] = app.store.mode_temps();
    Json(v)
}

/// The last read in Prometheus text format, for a scrape job (PROMETHEUS=0 turns it off).
async fn prometheus(State(app): State<AppState>) -> Response {
    if !app.prometheus {
        return StatusCode::NOT_FOUND.into_response();
    }
    let snap = app.snapshot.read().unwrap().clone();
    (
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        prometheus_text(&snap, app.device.calls()),
    )
        .into_response()
}

/// Device an input or output belongs to: "116.B" -> "°CALEONbox"; a bare "1" is the thermostat.
fn device_of<'a>(snap: &'a crate::app::Snapshot, label: &str) -> &'a str {
    match label.split_once('.') {
        Some((id, _)) if id.chars().all(|c| c.is_ascii_digit()) => snap
            .devices
            .get(id)
            .and_then(Value::as_str)
            .unwrap_or("CAN"),
        _ => "°CALEON",
    }
}

fn prometheus_text(snap: &crate::app::Snapshot, device_calls: (u64, usize)) -> String {
    type Labels<'a> = Vec<(&'a str, &'a str)>;
    fn esc(s: &str) -> String {
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    }
    fn labels(l: &[(&str, &str)]) -> String {
        if l.is_empty() {
            return String::new();
        }
        format!(
            "{{{}}}",
            l.iter()
                .map(|(k, v)| format!("{k}=\"{}\"", esc(v)))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
    let mut out = String::new();
    let mut metric = |name: &str, help: &str, samples: &[(Labels, f64)]| {
        if samples.is_empty() {
            return;
        }
        out.push_str(&format!("# HELP {name} {help}\n# TYPE {name} gauge\n"));
        for (l, v) in samples {
            out.push_str(&format!("{name}{} {v}\n", labels(l)));
        }
    };
    let ok = snap.error.is_none() && snap.updated.is_some();
    metric(
        "caleon_up",
        "1 when the last read of the thermostat succeeded.",
        &[(vec![], if ok { 1.0 } else { 0.0 })],
    );
    if let Some(t) = snap.updated {
        metric(
            "caleon_last_read_timestamp_seconds",
            "Unix time of the last successful read.",
            &[(vec![], t as f64)],
        );
    }
    let ids: Vec<String> = snap.rooms.iter().map(|r| r.id.to_string()).collect();
    let (mut temp, mut hum, mut set, mut mode, mut heating, mut outputs) =
        (vec![], vec![], vec![], vec![], vec![], vec![]);
    for (r, id) in snap.rooms.iter().zip(&ids) {
        let base: Labels = vec![("id", id), ("room", &r.name), ("type", &r.kind)];
        if let Some(v) = parse_number(&r.temperature) {
            temp.push((base.clone(), v));
        }
        if let Some(v) = parse_number(&r.humidity) {
            hum.push((base.clone(), v));
        }
        if let Some(st) = &r.state {
            if let Some(v) = parse_number(&st.tset) {
                set.push((base.clone(), v));
            }
            let cur = st.mode.split(' ').next().unwrap_or("");
            for m in ROOM_MODES {
                let mut l = base.clone();
                l.push(("mode", m));
                mode.push((l, if m == cur { 1.0 } else { 0.0 }));
            }
            let on = r.outputs.iter().any(|o| o["on"].as_bool().unwrap_or(false));
            heating.push((base.clone(), if on { 1.0 } else { 0.0 }));
        }
        for o in &r.outputs {
            let mut l = base.clone();
            l.push(("zone", o["zone"].as_str().unwrap_or("")));
            l.push(("output", o["label"].as_str().unwrap_or("")));
            l.push(("device", device_of(snap, o["label"].as_str().unwrap_or(""))));
            outputs.push((
                l,
                if o["on"].as_bool().unwrap_or(false) {
                    1.0
                } else {
                    0.0
                },
            ));
        }
    }
    metric(
        "caleon_room_temperature_celsius",
        "Room temperature.",
        &temp,
    );
    metric(
        "caleon_room_humidity_percent",
        "Room relative humidity, where the room has a humidity sensor.",
        &hum,
    );
    metric(
        "caleon_room_setpoint_celsius",
        "Room setpoint; absent while the room is off.",
        &set,
    );
    metric(
        "caleon_room_mode",
        "1 for the room's active mode (off, regular, eco, turbo), 0 for the others.",
        &mode,
    );
    metric(
        "caleon_room_heating",
        "1 while a zone valve of the room is open.",
        &heating,
    );
    metric(
        "caleon_output_on",
        "Zone valves of the central unit, by room.",
        &outputs,
    );
    let status: Vec<(Labels, f64)> = snap
        .status
        .iter()
        .filter_map(|m| {
            m.value.map(|v| {
                (
                    vec![
                        ("key", m.key.as_str()),
                        ("group", m.group.as_str()),
                        ("label", m.label.as_str()),
                        ("unit", m.unit.as_str()),
                        ("device", device_of(snap, &m.label)),
                    ],
                    v,
                )
            })
        })
        .collect();
    metric("caleon_status", "Every numeric value of the device status screens: sensors, outputs, circuit pump, boiler demand.", &status);
    let (calls, last_minute) = device_calls;
    out.push_str(&format!(
        "# HELP caleon_device_calls_total Queries sent to the thermostat since start.\n# TYPE caleon_device_calls_total counter\ncaleon_device_calls_total {calls}\n# HELP caleon_device_calls_per_minute Queries sent to the thermostat in the last minute.\n# TYPE caleon_device_calls_per_minute gauge\ncaleon_device_calls_per_minute {last_minute}\n"
    ));
    out
}

async fn refresh(State(app): State<AppState>) -> ApiResult {
    blocking(&app, |a| a.refresh(true)).await?;
    Ok(state(State(app)).await)
}

#[derive(Deserialize)]
struct HoursQuery {
    hours: Option<f64>,
}

async fn history(State(app): State<AppState>, Query(q): Query<HoursQuery>) -> ApiResult {
    let since = now() - (q.hours.unwrap_or(24.0) * 3600.0) as i64;
    Ok(Json(
        blocking(&app, move |a| a.store.room_history(since)).await?,
    ))
}

async fn metrics(State(app): State<AppState>, Query(q): Query<HoursQuery>) -> ApiResult {
    let since = now() - (q.hours.unwrap_or(24.0) * 3600.0) as i64;
    Ok(Json(
        blocking(&app, move |a| a.store.metric_history(since, 600)).await?,
    ))
}

/// What the server did, newest first: changes sent to the thermostat, backups, outages.
async fn events(
    State(app): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    let limit = q
        .get("limit")
        .and_then(|l| l.parse().ok())
        .unwrap_or(300usize)
        .min(2000);
    Ok(Json(json!(
        blocking(&app, move |a| a.store.events(limit)).await?
    )))
}

async fn message_log(State(app): State<AppState>) -> ApiResult {
    Ok(Json(blocking(&app, |a| a.device.message_log()).await?))
}

fn check_room(app: &App, id: u16) -> Result<(), ApiError> {
    let snap = app.snapshot.read().unwrap();
    if id == SYSTEM_ROOM && snap.system.is_some() || snap.rooms.iter().any(|r| r.id == id) {
        Ok(())
    } else {
        Err(ApiError(StatusCode::NOT_FOUND, format!("no room {id}")))
    }
}

/// Body: one of {"tdelta": ±1}, {"mode": ...}, {"name": ...}, {"season": ...},
/// {"vacation": "yyyy-MM-dd HH:mm"} or {"vacation": null} to cancel.
async fn room_change(
    State(app): State<AppState>,
    Path(id): Path<u16>,
    Json(body): Json<Value>,
) -> ApiResult {
    guard_write(&app)?;
    check_room(&app, id)?;
    let mut change = RoomChange::default();
    let mut cancel_holiday = false;
    if let Some(d) = body.get("tdelta") {
        let d = d
            .as_i64()
            .filter(|d| *d == 1 || *d == -1)
            .ok_or_else(|| bad("tdelta must be 1 or -1"))?;
        change.tdelta = Some(d as i8);
    } else if let Some(m) = body.get("mode") {
        let m = m
            .as_str()
            .filter(|m| ROOM_MODES.contains(m))
            .ok_or_else(|| bad("mode must be off, regular, eco or turbo"))?;
        change.mode = Some(m.into());
    } else if let Some(n) = body.get("name") {
        let n = n
            .as_str()
            .map(str::trim)
            .filter(|n| !n.is_empty() && n.len() <= 32)
            .ok_or_else(|| bad("invalid name"))?;
        change.name = Some(n.into());
    } else if let Some(s) = body.get("season") {
        let s = s
            .as_str()
            .filter(|s| *s == "heating" || *s == "cooling")
            .ok_or_else(|| bad("season must be heating or cooling"))?;
        change.season = Some(s.into());
    } else if let Some(v) = body.get("vacation") {
        match v.as_str() {
            Some(s) => {
                let ok = s.len() == 16
                    && s.chars().enumerate().all(|(i, c)| match i {
                        4 | 7 => c == '-',
                        10 => c == ' ',
                        13 => c == ':',
                        _ => c.is_ascii_digit(),
                    });
                if !ok {
                    return Err(bad("vacation must be 'yyyy-MM-dd HH:mm'"));
                }
                change.vacation = Some(s.into());
            }
            None => cancel_holiday = true,
        }
    } else {
        return Err(bad("nothing to change"));
    }
    tracing::info!("room {id} change {body}");
    let st = blocking(&app, move |a| {
        if cancel_holiday {
            // the app cancels holiday mode by re-sending the current mode
            let cur = a.device.room_state(id, &RoomChange::default())?;
            let mode = cur
                .mode
                .split(' ')
                .next()
                .filter(|m| !m.is_empty())
                .unwrap_or("regular")
                .to_string();
            change.mode = Some(mode);
        }
        let st = a.device.room_state(id, &change)?;
        a.apply_room_state(id, st.clone());
        // "all rooms" changes every room, so read them all before answering and the answer shows it.
        // A single room answers right away; its valves and the rest follow in the background.
        if id == SYSTEM_ROOM {
            if let Err(e) = a.refresh(false) {
                tracing::debug!("reread after change: {e}");
            }
        }
        Ok(st)
    })
    .await?;
    let what = if let Some(d) = body.get("tdelta").and_then(Value::as_i64) {
        format!(
            "Consigna {} → {}",
            if d > 0 { "+0,5" } else { "−0,5" },
            st.tset.replace('.', ",")
        )
    } else if let Some(m) = body.get("mode").and_then(Value::as_str) {
        format!("Modo {}", mode_label(m))
    } else if let Some(n) = body.get("name").and_then(Value::as_str) {
        format!("Nombre cambiado a «{}»", n.trim())
    } else if let Some(s) = body.get("season").and_then(Value::as_str) {
        format!(
            "Estación: {}",
            if s == "cooling" {
                "refrigeración"
            } else {
                "calefacción"
            }
        )
    } else if body.get("vacation").map(Value::is_null).unwrap_or(false) {
        "Vacaciones terminadas".to_string()
    } else if let Some(v) = body.get("vacation").and_then(Value::as_str) {
        format!("Vacaciones hasta {v}")
    } else {
        "Cambio".to_string()
    };
    if id == SYSTEM_ROOM {
        app.log("web", None, format!("Toda la casa: {what}"));
    } else {
        app.log("web", Some(id), what);
    }
    reread_later(&app, (id != SYSTEM_ROOM).then_some(id), true);
    let mut v = serde_json::to_value(st).unwrap();
    v["state"] = state(State(app)).await.0;
    Ok(Json(v))
}

#[derive(Deserialize)]
struct ModeTempsBody {
    regular: Option<f64>,
    eco: Option<f64>,
    turbo: Option<f64>,
    /// Also visit the modes not given, to learn their setpoints.
    #[serde(default)]
    read: bool,
}

/// Body: `{"regular": 21.5, "eco": 18, "turbo": 23, "read": true}`, any subset. The room visits each
/// mode briefly and comes back to the mode it had.
async fn room_modes(
    State(app): State<AppState>,
    Path(id): Path<u16>,
    Json(b): Json<ModeTempsBody>,
) -> ApiResult {
    guard_write(&app)?;
    check_room(&app, id)?;
    if id == SYSTEM_ROOM {
        return Err(bad("mode temperatures are set per room"));
    }
    let mut targets = Vec::new();
    for (mode, v) in [("regular", b.regular), ("eco", b.eco), ("turbo", b.turbo)] {
        if let Some(v) = v {
            if !(5.0..=30.0).contains(&v) || (v * 2.0).fract() != 0.0 {
                return Err(bad("temperatures go from 5 to 30 in half-degree steps"));
            }
            targets.push((mode.to_string(), v));
        }
    }
    if targets.is_empty() && !b.read {
        return Err(bad("nothing to change"));
    }
    tracing::info!("room {id} mode temps {targets:?} read={}", b.read);
    let read = b.read;
    let said: Vec<String> = targets
        .iter()
        .map(|(m, v)| format!("{} {}", mode_label(m), fmt_temp(*v)))
        .collect();
    let seen = blocking(&app, move |a| a.set_mode_temps(id, &targets, read)).await?;
    if said.is_empty() {
        let got: Vec<String> = seen
            .iter()
            .filter_map(|(m, v)| {
                v.as_f64()
                    .map(|v| format!("{} {}", mode_label(m), fmt_temp(v)))
            })
            .collect();
        app.log(
            "web",
            Some(id),
            format!("Temperaturas de los modos leídas: {}", got.join(" · ")),
        );
    } else {
        app.log(
            "web",
            Some(id),
            format!("Temperaturas de los modos: {}", said.join(" · ")),
        );
    }
    reread_later(&app, Some(id), true);
    Ok(Json(
        json!({"temps": seen, "mode_temps": app.store.mode_temps()}),
    ))
}

#[derive(Deserialize)]
struct TimerQuery {
    #[serde(rename = "type")]
    kind: Option<u8>,
}

fn timer_type(id: u16, q: &TimerQuery) -> u8 {
    if id == SYSTEM_ROOM {
        2
    } else {
        q.kind.unwrap_or(0).min(1)
    }
}

fn week_json(id: u16, values: &[u32]) -> Value {
    let modes = device::timer_modes(id);
    json!({"modes": modes, "days": device::decode_week(values, modes)})
}

async fn timer_get(
    State(app): State<AppState>,
    Path(id): Path<u16>,
    Query(q): Query<TimerQuery>,
) -> ApiResult {
    check_room(&app, id)?;
    let t = timer_type(id, &q);
    let v = blocking(&app, move |a| a.device.room_timer(id, t, None)).await?;
    Ok(Json(week_json(id, &v)))
}

#[derive(Deserialize)]
struct DaySlots {
    slots: Vec<String>,
}

async fn timer_put(
    State(app): State<AppState>,
    Path((id, day)): Path<(u16, u8)>,
    Query(q): Query<TimerQuery>,
    Json(body): Json<DaySlots>,
) -> ApiResult {
    guard_write(&app)?;
    check_room(&app, id)?;
    if day > 6 {
        return Err(bad("day must be 0 (Monday) to 6"));
    }
    let bf =
        device::bitfield(&body.slots, device::timer_modes(id)).map_err(|e| bad(e.to_string()))?;
    let t = timer_type(id, &q);
    tracing::info!("room {id} timer type {t} day {day} set");
    let v = blocking(&app, move |a| a.device.room_timer(id, t, Some((day, bf)))).await?;
    app.log(
        "web",
        (id != SYSTEM_ROOM).then_some(id),
        format!(
            "Horario del {} guardado{}",
            crate::app::DAYS[day as usize],
            timer_kind(id, t)
        ),
    );
    app.snapshot.write().unwrap().weeks_at = None;
    reread_later(&app, None, false);
    Ok(Json(week_json(id, &v)))
}

#[derive(Deserialize)]
struct CopyBody {
    mask: u8,
}

async fn timer_copy(
    State(app): State<AppState>,
    Path((id, day)): Path<(u16, u8)>,
    Query(q): Query<TimerQuery>,
    Json(body): Json<CopyBody>,
) -> ApiResult {
    guard_write(&app)?;
    check_room(&app, id)?;
    if day > 6 || body.mask == 0 || body.mask > 127 {
        return Err(bad("invalid day or mask"));
    }
    let t = timer_type(id, &q);
    tracing::info!(
        "room {id} timer type {t} copy day {day} to mask {:#09b}",
        body.mask
    );
    let v = blocking(&app, move |a| {
        a.device.room_timer_copy(id, t, day, body.mask)
    })
    .await?;
    let to: Vec<&str> = (0..7)
        .filter(|d| body.mask >> d & 1 == 1)
        .map(|d| crate::app::DAYS[d])
        .collect();
    app.log(
        "web",
        (id != SYSTEM_ROOM).then_some(id),
        format!(
            "Horario del {} copiado a {}{}",
            crate::app::DAYS[day as usize],
            to.join(", "),
            timer_kind(id, t)
        ),
    );
    app.snapshot.write().unwrap().weeks_at = None;
    reread_later(&app, None, false);
    Ok(Json(week_json(id, &v)))
}

#[derive(Deserialize)]
struct MenuActionBody {
    id: String,
    action: String,
}

/// Sends a menu action (navigation, +/-, selection, yes/no...) and returns the new screen.
async fn menu_action(State(app): State<AppState>, Json(b): Json<MenuActionBody>) -> ApiResult {
    if !is_navigation(&b.action) {
        guard_write(&app)?;
    }
    if app.crawling.load(Ordering::SeqCst) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Se está leyendo la configuración del equipo, espera a que termine".into(),
        ));
    }
    tracing::info!("menu action {:?} on {:?}", b.action, b.id);
    let writes = !is_navigation(&b.action);
    let b_action = b.action.clone();
    let v = blocking(&app, move |a| {
        let ext = a.extended();
        let menu = a.device.menu_action(ext, &b.id, &b.action)?;
        let kind = menu["type"].as_str().unwrap_or("").to_string();
        let id = menu["id"].as_str().unwrap_or("").to_string();
        let detail = if kind.is_empty() {
            Value::Null
        } else {
            a.device.menu_detail(ext, &kind, &id)?
        };
        Ok(json!({"menu": menu, "detail": detail}))
    })
    .await?;
    if writes {
        app.log(
            "web",
            None,
            format!(
                "Experto: {} en «{}»",
                b_action,
                v["menu"]["title"].as_str().unwrap_or("?")
            ),
        );
        reread_later(&app, None, false);
    }
    Ok(Json(v))
}

async fn menu_detail(
    State(app): State<AppState>,
    Query(q): Query<HashMap<String, String>>,
) -> ApiResult {
    let id = q.get("id").cloned().ok_or_else(|| bad("missing id"))?;
    let kind = q.get("type").cloned().ok_or_else(|| bad("missing type"))?;
    Ok(Json(
        blocking(&app, move |a| {
            a.device.menu_detail(a.extended(), &kind, &id)
        })
        .await?,
    ))
}

async fn menu_set(State(app): State<AppState>, Json(b): Json<Value>) -> ApiResult {
    guard_write(&app)?;
    let id = b["id"]
        .as_str()
        .ok_or_else(|| bad("missing id"))?
        .to_string();
    let kind = b["type"]
        .as_str()
        .ok_or_else(|| bad("missing type"))?
        .to_string();
    tracing::info!("menu set {kind} {id}: {b}");
    let said = format!("Experto: {kind} guardado en {id}");
    let v = blocking(&app, move |a| a.device.menu_set(&kind, &id, &b)).await?;
    app.log("web", None, said);
    reread_later(&app, None, false);
    Ok(Json(v))
}

async fn backups_list(State(app): State<AppState>) -> ApiResult {
    Ok(Json(json!(blocking(&app, |a| a.list_backups()).await?)))
}

async fn backup_create(State(app): State<AppState>) -> ApiResult {
    if app.crawling.load(Ordering::SeqCst) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Ya hay una copia en curso".into(),
        ));
    }
    let name = blocking(&app, |a| a.create_backup("manual", false)).await?;
    app.log(
        "web",
        None,
        format!("Copia manual guardada ({})", name.as_deref().unwrap_or("?")),
    );
    Ok(Json(json!({"name": name})))
}

async fn backup_get(
    State(app): State<AppState>,
    Path(name): Path<String>,
) -> Result<Response, ApiError> {
    let n = name.clone();
    let v = blocking(&app, move |a| a.read_backup(&n))
        .await
        .map_err(|e| ApiError(StatusCode::NOT_FOUND, e.1))?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/json".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        serde_json::to_vec_pretty(&v).unwrap(),
    )
        .into_response())
}

async fn backup_delete(State(app): State<AppState>, Path(name): Path<String>) -> ApiResult {
    guard_write(&app)?;
    let said = format!("Copia {name} borrada");
    blocking(&app, move |a| {
        a.read_backup(&name)?;
        Ok(std::fs::remove_file(a.backups_dir().join(&name))?)
    })
    .await?;
    app.log("web", None, said);
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct RestoreBody {
    apply: bool,
    #[serde(default = "yes")]
    names: bool,
    #[serde(default = "yes")]
    timers: bool,
    /// Optional uploaded backup; otherwise the named file is used.
    backup: Option<Value>,
}

fn yes() -> bool {
    true
}

async fn backup_restore(
    State(app): State<AppState>,
    Path(name): Path<String>,
    Json(b): Json<RestoreBody>,
) -> ApiResult {
    if b.apply {
        guard_write(&app)?;
    }
    if app.crawling.load(Ordering::SeqCst) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Ya hay una copia en curso".into(),
        ));
    }
    if let Some(up) = &b.backup {
        if up["format"]
            .as_str()
            .is_none_or(|f| !f.starts_with("caleon-backup/"))
            || !up["rooms"].is_array()
        {
            return Err(bad(
                "Ese archivo no es una copia de seguridad de esta aplicación",
            ));
        }
    }
    tracing::info!(
        "restore {name} apply={} names={} timers={}",
        b.apply,
        b.names,
        b.timers
    );
    let apply = b.apply;
    let backup_name = name.clone();
    let v = blocking(&app, move |a| {
        let backup = match b.backup {
            Some(v) => v,
            None => a.read_backup(&name)?,
        };
        a.restore(&backup, b.apply, b.names, b.timers)
    })
    .await?;
    if apply {
        app.log(
            "web",
            None,
            format!(
                "Copia {} restaurada: {} cambios",
                backup_name,
                v["changes"].as_array().map_or(0, Vec::len)
            ),
        );
        app.snapshot.write().unwrap().weeks_at = None;
        reread_later(&app, None, false);
    }
    Ok(Json(v))
}

fn settings_json(app: &App) -> Value {
    let (settings, found) = &*app.conn.read().unwrap();
    json!({
        "transport": settings.transport,
        "host": settings.host,
        "email": settings.email,
        "active": app.device.transport().kind(),
        "available": crate::connection::available(),
        "device": found.id,
        "addr": found.addr.to_string(),
    })
}

async fn settings_get(State(app): State<AppState>) -> Json<Value> {
    Json(settings_json(&app))
}

#[derive(Deserialize)]
struct SettingsBody {
    transport: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    email: String,
}

/// Switches the connection mode: locates the device, checks the new transport answers and only
/// then replaces the current one and saves the settings.
async fn settings_set(State(app): State<AppState>, Json(b): Json<SettingsBody>) -> ApiResult {
    guard_write(&app)?;
    if app.crawling.load(Ordering::SeqCst) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Hay una copia en curso, espera a que termine".into(),
        ));
    }
    let host = b.host.trim().to_string();
    tracing::info!(
        "settings: transport={} host={host:?} email={:?}",
        b.transport,
        b.email
    );
    blocking(&app, move |a| {
        let want = a.device.id.clone();
        let found = caleon::locate(&host, &want)?;
        let settings = crate::connection::Settings {
            transport: b.transport.clone(),
            host,
            email: b.email.trim().to_string(),
        };
        let t = crate::connection::build(&settings, &found, &a.schema, &a.data_dir)?;
        t.invoke("versions.json", &[], true)
            .map_err(|e| anyhow::anyhow!("el modo «{}» no respondió: {e}", b.transport))?;
        a.device.set_transport(t);
        settings.save(&a.data_dir)?;
        *a.conn.write().unwrap() = (settings, found);
        Ok(())
    })
    .await?;
    app.log(
        "web",
        None,
        format!("Modo de conexión: {}", app.device.transport().kind()),
    );
    Ok(Json(settings_json(&app)))
}

#[derive(Deserialize)]
struct MenuOpenBody {
    path: Vec<String>,
}

/// Opens the device menu at a path of entry titles (used to restore the screen after a reload).
async fn menu_open(State(app): State<AppState>, Json(b): Json<MenuOpenBody>) -> ApiResult {
    if app.crawling.load(Ordering::SeqCst) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Se está leyendo la configuración del equipo, espera a que termine".into(),
        ));
    }
    let v = blocking(&app, move |a| {
        let ext = a.extended();
        let menu = a.renavigate(ext, &b.path)?;
        let kind = menu["type"].as_str().unwrap_or("").to_string();
        let id = menu["id"].as_str().unwrap_or("").to_string();
        let detail = if kind.is_empty() {
            Value::Null
        } else {
            a.device.menu_detail(ext, &kind, &id)?
        };
        Ok(json!({"menu": menu, "detail": detail}))
    })
    .await?;
    Ok(Json(v))
}

/// Value shown on each menu screen, from the latest backup: {"Ajustes › Zonas › Zona B › Día V/I": "21.0°C"}.
async fn menu_values(State(app): State<AppState>) -> ApiResult {
    Ok(Json(blocking(&app, |a| {
        let Some(name) = a.list_backups()?.first().and_then(|b| b["name"].as_str().map(String::from)) else {
            return Ok(json!({}));
        };
        let backup = a.read_backup(&name)?;
        let mut flat = Vec::new();
        crate::app::flatten_values(&backup["menu"], &mut flat);
        Ok(json!({"created": backup["created"], "values": flat.into_iter().map(|(k, v)| (k, Value::String(v))).collect::<serde_json::Map<String, Value>>()}))
    })
    .await?))
}

async fn weather_get(State(app): State<AppState>) -> Json<crate::weather::runtime::WeatherView> {
    Json(app.weather.view())
}

async fn weather_set(
    State(app): State<AppState>,
    Json(input): Json<crate::weather::runtime::WeatherUpdate>,
) -> ApiResult {
    let view = app.weather.save(&app, input).await.map_err(weather_error)?;
    Ok(Json(
        serde_json::to_value(view).map_err(anyhow::Error::from)?,
    ))
}

#[derive(Deserialize)]
struct WeatherLocationQuery {
    name: String,
}

async fn weather_locations(
    State(app): State<AppState>,
    Query(query): Query<WeatherLocationQuery>,
) -> ApiResult {
    let locations = crate::weather::open_meteo::OpenMeteo::new(app.weather.client.clone())
        .locations(&query.name)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(Json(
        serde_json::to_value(locations).map_err(anyhow::Error::from)?,
    ))
}

#[derive(Deserialize)]
struct WeatherStationsQuery {
    location: crate::weather::Coordinates,
    aemet_api_key: Option<String>,
}

async fn weather_stations(
    State(app): State<AppState>,
    Json(query): Json<WeatherStationsQuery>,
) -> ApiResult {
    use crate::weather::WeatherProvider;
    query.location.validate().map_err(weather_error)?;
    let key = app
        .weather
        .configured_key(query.aemet_api_key)
        .map_err(weather_error)?;
    let stations = crate::weather::aemet::Aemet::new(app.weather.client.clone(), key)
        .stations(query.location)
        .await
        .map_err(anyhow::Error::from)?;
    Ok(Json(
        serde_json::to_value(stations).map_err(anyhow::Error::from)?,
    ))
}

async fn weather_history(
    State(app): State<AppState>,
    Query(query): Query<HoursQuery>,
) -> ApiResult {
    let hours = query.hours.unwrap_or(24.0);
    if !hours.is_finite() || hours <= 0.0 || hours > 720.0 {
        return Err(bad("El periodo debe estar entre 0 y 720 horas"));
    }
    let range = crate::weather::TimeRange {
        start: (now() - (hours * 3600.0) as i64).max(0),
        end: now() + 1,
    };
    Ok(Json(
        blocking(&app, move |app| {
            Ok(serde_json::to_value(
                app.store.weather_history(range, true)?,
            )?)
        })
        .await?,
    ))
}

async fn weather_recover(State(app): State<AppState>) -> ApiResult {
    app.weather.request_recovery().map_err(weather_error)?;
    Ok(Json(json!({"started":true})))
}

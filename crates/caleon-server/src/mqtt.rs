//! Home Assistant over MQTT: discovery messages so HA creates the entities on its own (a
//! climate per room, sensors, the pump and boiler demand, the whole-house mode), retained state
//! after every read, and commands (mode, preset, setpoint) back to the thermostat.
//!
//! Topics under `MQTT_TOPIC` (default `caleon`):
//! `room/<id>/{mode,preset,temperature,humidity,target,heating,action}` for state,
//! `room/<id>/set/{mode,preset,temperature}` and `house/set/mode` for commands,
//! `pump`, `boiler`, `house/mode`, `last_read` and `status` (online/offline, last will).

use crate::app::{parse_number, timestamp_name, App, Snapshot};
use anyhow::{anyhow, bail, Result};
use caleon::device::{RoomChange, SYSTEM_ROOM};
use rumqttc::{AsyncClient, Event, LastWill, MqttOptions, Packet, QoS};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub host: String,
    pub port: u16,
    pub user: Option<String>,
    pub password: Option<String>,
    /// Base of every topic.
    pub base: String,
    /// Home Assistant's discovery prefix.
    pub discovery_prefix: String,
    pub client_id: String,
}

impl Config {
    /// `MQTT_URL=mqtt://[user[:password]@]host[:1883]`; `None` when the variable is not set.
    pub fn from_env() -> Result<Option<Config>> {
        let Ok(url) = std::env::var("MQTT_URL") else {
            return Ok(None);
        };
        let mut c = Config::parse(&url)?;
        if let Ok(v) = std::env::var("MQTT_TOPIC") {
            c.base = v.trim_matches('/').to_string();
        }
        if let Ok(v) = std::env::var("MQTT_DISCOVERY_PREFIX") {
            c.discovery_prefix = v.trim_matches('/').to_string();
        }
        if let Ok(v) = std::env::var("MQTT_CLIENT_ID") {
            c.client_id = v;
        }
        Ok(Some(c))
    }

    pub fn parse(url: &str) -> Result<Config> {
        let rest = url
            .strip_prefix("mqtt://")
            .ok_or_else(|| {
                anyhow!("MQTT_URL must look like mqtt://user:password@host:1883 (no TLS)")
            })?
            .trim_end_matches('/');
        let (auth, hostport) = match rest.rsplit_once('@') {
            Some((a, h)) => (Some(a), h),
            None => (None, rest),
        };
        let (user, password) = match auth {
            Some(a) => match a.split_once(':') {
                Some((u, p)) => (Some(pct_decode(u)), Some(pct_decode(p))),
                None => (Some(pct_decode(a)), None),
            },
            None => (None, None),
        };
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) => (
                h.to_string(),
                p.parse().map_err(|_| anyhow!("MQTT_URL: bad port {p:?}"))?,
            ),
            None => (hostport.to_string(), 1883),
        };
        if host.is_empty() {
            bail!("MQTT_URL: missing host");
        }
        Ok(Config {
            host,
            port,
            user,
            password,
            base: "caleon".into(),
            discovery_prefix: "homeassistant".into(),
            client_id: "caleon".into(),
        })
    }
}

/// `%40` -> `@`, so passwords with reserved characters fit in the URL.
fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---- what Home Assistant sees

/// Home Assistant's hvac mode and preset for a device mode ("turbo 01:23" -> heat, comfort).
pub fn hvac(mode: &str) -> (&'static str, &'static str) {
    match mode.split(' ').next().unwrap_or("") {
        "off" => ("off", "none"),
        "eco" => ("heat", "eco"),
        "turbo" => ("heat", "comfort"),
        _ => ("heat", "none"),
    }
}

const HOUSE_MODES: [(&str, &str); 4] = [
    ("off", "Apagado"),
    ("regular", "Normal"),
    ("eco", "Eco"),
    ("turbo", "Confort"),
];

/// Shown when the rooms are not all in the same mode; selecting it changes nothing.
const HOUSE_MIXED: &str = "Mixto";

fn house_label(mode: &str) -> &'static str {
    let m = mode.split(' ').next().unwrap_or("");
    HOUSE_MODES
        .iter()
        .find(|(k, _)| *k == m)
        .map(|(_, l)| *l)
        .unwrap_or(HOUSE_MIXED)
}

fn house_mode(label: &str) -> Option<&'static str> {
    HOUSE_MODES
        .iter()
        .find(|(_, l)| l.eq_ignore_ascii_case(label.trim()))
        .map(|(k, _)| *k)
}

/// "20f85e6e1ed1.sorel-connect.net" -> "20f85e6e1ed1", safe inside topics and ids.
fn device_slug(device: &str) -> String {
    let s: String = device
        .split('.')
        .next()
        .unwrap_or(device)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if s.is_empty() {
        "device".into()
    } else {
        s.to_lowercase()
    }
}

fn merge(into: &mut Value, extra: Value) {
    if let (Some(a), Some(b)) = (into.as_object_mut(), extra.as_object()) {
        for (k, v) in b {
            a.insert(k.clone(), v.clone());
        }
    }
}

fn output(snap: &Snapshot, needle: &str) -> Option<f64> {
    snap.status
        .iter()
        .find(|m| m.group == "Salidas" && m.label.contains(needle))
        .and_then(|m| m.value)
}

fn on_off(v: bool) -> &'static str {
    if v {
        "ON"
    } else {
        "OFF"
    }
}

/// Discovery messages (topic, retained JSON) for everything in the snapshot.
pub fn discovery(snap: &Snapshot, cfg: &Config) -> Vec<(String, Value)> {
    let slug = device_slug(&snap.device);
    let (base, p) = (&cfg.base, &cfg.discovery_prefix);
    let device = json!({
        "identifiers": [format!("caleon_{slug}")],
        "name": "Caleon",
        "manufacturer": "SOREL",
        "model": "°CALEON",
        "sw_version": snap.firmware.clone().unwrap_or_default(),
    });
    let common = |uid: &str, name: &str| {
        json!({
            "unique_id": format!("caleon_{slug}_{uid}"),
            "name": name,
            "device": device,
            "availability_topic": format!("{base}/status"),
        })
    };
    let mut out = Vec::new();
    for r in &snap.rooms {
        if r.state.is_none() {
            continue;
        }
        let id = r.id;
        let t = format!("{base}/room/{id}");
        let humidity = parse_number(&r.humidity).is_some();
        let mut c = common(&format!("room{id}"), &r.name);
        merge(
            &mut c,
            json!({
                "modes": ["heat", "off"],
                "mode_state_topic": format!("{t}/mode"),
                "mode_command_topic": format!("{t}/set/mode"),
                "preset_modes": ["eco", "comfort"],
                "preset_mode_state_topic": format!("{t}/preset"),
                "preset_mode_command_topic": format!("{t}/set/preset"),
                "current_temperature_topic": format!("{t}/temperature"),
                "temperature_state_topic": format!("{t}/target"),
                "temperature_command_topic": format!("{t}/set/temperature"),
                "action_topic": format!("{t}/action"),
                "temp_step": 0.5,
                "min_temp": 5,
                "max_temp": 30,
                "precision": 0.1,
                "temperature_unit": "C",
            }),
        );
        if humidity {
            c["current_humidity_topic"] = json!(format!("{t}/humidity"));
        }
        out.push((format!("{p}/climate/caleon_{slug}/room{id}/config"), c));

        let mut s = common(
            &format!("room{id}_temperature"),
            &format!("{} temperatura", r.name),
        );
        merge(
            &mut s,
            json!({"state_topic": format!("{t}/temperature"), "device_class": "temperature", "unit_of_measurement": "°C", "state_class": "measurement"}),
        );
        out.push((
            format!("{p}/sensor/caleon_{slug}/room{id}_temperature/config"),
            s,
        ));
        if humidity {
            let mut s = common(
                &format!("room{id}_humidity"),
                &format!("{} humedad", r.name),
            );
            merge(
                &mut s,
                json!({"state_topic": format!("{t}/humidity"), "device_class": "humidity", "unit_of_measurement": "%", "state_class": "measurement"}),
            );
            out.push((
                format!("{p}/sensor/caleon_{slug}/room{id}_humidity/config"),
                s,
            ));
        }
        let mut b = common(
            &format!("room{id}_heating"),
            &format!("{} calentando", r.name),
        );
        merge(
            &mut b,
            json!({"state_topic": format!("{t}/heating"), "device_class": "heat"}),
        );
        out.push((
            format!("{p}/binary_sensor/caleon_{slug}/room{id}_heating/config"),
            b,
        ));
    }
    for (uid, name, needle, class) in [
        ("pump", "Bomba del circuito", "Bomba", "running"),
        ("boiler", "Demanda a la caldera", "Demanda", "heat"),
    ] {
        if output(snap, needle).is_some() {
            let mut b = common(uid, name);
            merge(
                &mut b,
                json!({"state_topic": format!("{base}/{uid}"), "device_class": class}),
            );
            out.push((format!("{p}/binary_sensor/caleon_{slug}/{uid}/config"), b));
        }
    }
    if snap.system.is_some() {
        let mut s = common("house", "Toda la casa");
        merge(
            &mut s,
            json!({
                "options": HOUSE_MODES.iter().map(|(_, l)| *l).chain([HOUSE_MIXED]).collect::<Vec<_>>(),
                "state_topic": format!("{base}/house/mode"),
                "command_topic": format!("{base}/house/set/mode"),
                "icon": "mdi:home-thermometer",
            }),
        );
        out.push((format!("{p}/select/caleon_{slug}/house/config"), s));
    }
    let mut s = common("last_read", "Última lectura");
    merge(
        &mut s,
        json!({"state_topic": format!("{base}/last_read"), "device_class": "timestamp", "entity_category": "diagnostic"}),
    );
    out.push((format!("{p}/sensor/caleon_{slug}/last_read/config"), s));
    out
}

/// State messages (topic, retained payload) for the snapshot.
pub fn states(snap: &Snapshot, cfg: &Config) -> Vec<(String, String)> {
    let base = &cfg.base;
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |t: String, v: String| out.push((t, v));
    for r in &snap.rooms {
        let Some(st) = &r.state else { continue };
        let t = format!("{base}/room/{}", r.id);
        let (hvac_mode, preset) = hvac(&st.mode);
        push(format!("{t}/mode"), hvac_mode.into());
        push(format!("{t}/preset"), preset.into());
        if let Some(v) = parse_number(&r.temperature) {
            push(format!("{t}/temperature"), v.to_string());
        }
        if let Some(v) = parse_number(&r.humidity) {
            push(format!("{t}/humidity"), v.to_string());
        }
        // "None" clears the target in Home Assistant while the room is off
        push(
            format!("{t}/target"),
            parse_number(&st.tset)
                .map(|v| v.to_string())
                .unwrap_or_else(|| "None".into()),
        );
        let heating = r.outputs.iter().any(|o| o["on"].as_bool().unwrap_or(false));
        push(format!("{t}/heating"), on_off(heating).into());
        push(
            format!("{t}/action"),
            if hvac_mode == "off" {
                "off"
            } else if heating {
                "heating"
            } else {
                "idle"
            }
            .into(),
        );
    }
    for (uid, needle) in [("pump", "Bomba"), ("boiler", "Demanda")] {
        if let Some(v) = output(snap, needle) {
            push(format!("{base}/{uid}"), on_off(v != 0.0).into());
        }
    }
    if let Some(sys) = &snap.system {
        push(format!("{base}/house/mode"), house_label(&sys.mode).into());
    }
    if let Some(ts) = snap.updated {
        push(format!("{base}/last_read"), iso_utc(ts));
    }
    out
}

/// "2026-10-16T09:30:00+00:00"
fn iso_utc(ts: i64) -> String {
    let n = timestamp_name(ts); // YYYYmmdd-HHMMSS
    format!(
        "{}-{}-{}T{}:{}:{}+00:00",
        &n[0..4],
        &n[4..6],
        &n[6..8],
        &n[9..11],
        &n[11..13],
        &n[13..15]
    )
}

// ---- commands from Home Assistant

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// hvac mode: `heat` turns an off room to Normal, `off` switches it off.
    Hvac {
        id: u16,
        heat: bool,
    },
    /// preset: eco, comfort or none (Normal).
    Preset {
        id: u16,
        mode: &'static str,
    },
    Target {
        id: u16,
        target: f64,
    },
    House {
        mode: &'static str,
    },
}

pub fn parse_command(topic: &str, payload: &str, base: &str) -> Option<Command> {
    let rest = topic.strip_prefix(base)?.strip_prefix('/')?;
    let payload = payload.trim();
    if rest == "house/set/mode" {
        return house_mode(payload).map(|mode| Command::House { mode });
    }
    let mut parts = rest.split('/');
    if parts.next()? != "room" {
        return None;
    }
    let id: u16 = parts.next()?.parse().ok()?;
    if parts.next()? != "set" {
        return None;
    }
    match (parts.next()?, parts.next()) {
        ("mode", None) => match payload {
            "heat" => Some(Command::Hvac { id, heat: true }),
            "off" => Some(Command::Hvac { id, heat: false }),
            _ => None,
        },
        ("preset", None) => match payload {
            "eco" => Some(Command::Preset { id, mode: "eco" }),
            "comfort" => Some(Command::Preset { id, mode: "turbo" }),
            "none" => Some(Command::Preset {
                id,
                mode: "regular",
            }),
            _ => None,
        },
        ("temperature", None) => payload
            .parse()
            .ok()
            .map(|target| Command::Target { id, target }),
        _ => None,
    }
}

/// Half-degree steps from the current setpoint to the wanted one (the device only moves ±0.5).
pub fn steps(current: f64, target: f64) -> i32 {
    ((target - current) * 2.0).round() as i32
}

fn set_mode(app: &App, id: u16, mode: &str) -> Result<()> {
    let st = app.device.room_state(
        id,
        &RoomChange {
            mode: Some(mode.into()),
            ..Default::default()
        },
    )?;
    app.apply_room_state(id, st);
    Ok(())
}

/// Applies a command to the thermostat. Blocking.
pub fn execute(app: &App, cmd: &Command) -> Result<()> {
    match cmd {
        Command::Hvac { id, heat: false } => set_mode(app, *id, "off"),
        Command::Hvac { id, heat: true } => {
            let cur = app.device.room_state(*id, &RoomChange::default())?;
            if hvac(&cur.mode).0 == "off" {
                set_mode(app, *id, "regular")
            } else {
                Ok(())
            }
        }
        Command::Preset { id, mode } => set_mode(app, *id, mode),
        Command::House { mode } => set_mode(app, SYSTEM_ROOM, mode),
        Command::Target { id, target } => {
            let mut cur = app.device.room_state(*id, &RoomChange::default())?;
            let Some(mut now) = parse_number(&cur.tset) else {
                bail!("room {id} is off: switch it on before setting a temperature");
            };
            // one relative step at a time, reading the answer back, so a lost packet cannot overshoot
            for _ in 0..60 {
                let n = steps(now, *target);
                if n == 0 {
                    break;
                }
                cur = app.device.room_state(
                    *id,
                    &RoomChange {
                        tdelta: Some(n.signum() as i8),
                        ..Default::default()
                    },
                )?;
                app.apply_room_state(*id, cur.clone());
                now = parse_number(&cur.tset)
                    .ok_or_else(|| anyhow!("room {id} answered without a setpoint"))?;
            }
            Ok(())
        }
    }
}

// ---- the client

pub fn spawn(app: Arc<App>, cfg: Config) {
    tokio::spawn(run(app, cfg));
}

async fn run(app: Arc<App>, cfg: Config) {
    let mut opts = MqttOptions::new(cfg.client_id.clone(), cfg.host.clone(), cfg.port);
    opts.set_keep_alive(Duration::from_secs(30));
    if let Some(u) = &cfg.user {
        opts.set_credentials(u, cfg.password.clone().unwrap_or_default());
    }
    opts.set_last_will(LastWill::new(
        format!("{}/status", cfg.base),
        "offline",
        QoS::AtLeastOnce,
        true,
    ));
    let (client, mut eventloop) = AsyncClient::new(opts, 64);
    // bumped on every (re)connection: the publisher then resends discovery and state
    let session = Arc::new(AtomicU64::new(0));

    {
        let (client, app, cfg, session) =
            (client.clone(), app.clone(), cfg.clone(), session.clone());
        tokio::spawn(async move {
            let (mut seen, mut last_disc, mut last_state) = (0, String::new(), String::new());
            loop {
                let s = session.load(Ordering::SeqCst);
                if s != seen {
                    seen = s;
                    last_disc.clear();
                    last_state.clear();
                }
                let snap = app.snapshot.read().unwrap().clone();
                if s > 0 && snap.updated.is_some() {
                    let disc = discovery(&snap, &cfg);
                    let key = serde_json::to_string(&disc).unwrap_or_default();
                    if key != last_disc {
                        for (t, v) in &disc {
                            let _ = client
                                .publish(t, QoS::AtLeastOnce, true, v.to_string())
                                .await;
                        }
                        last_disc = key;
                    }
                    let st = states(&snap, &cfg);
                    let key = serde_json::to_string(&st).unwrap_or_default();
                    if key != last_state {
                        for (t, p) in &st {
                            let _ = client.publish(t, QoS::AtLeastOnce, true, p.clone()).await;
                        }
                        last_state = key;
                    }
                }
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        });
    }

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                tracing::info!("mqtt: connected to {}:{}", cfg.host, cfg.port);
                let _ = client
                    .subscribe(format!("{}/room/+/set/+", cfg.base), QoS::AtLeastOnce)
                    .await;
                let _ = client
                    .subscribe(format!("{}/house/set/mode", cfg.base), QoS::AtLeastOnce)
                    .await;
                let _ = client
                    .publish(
                        format!("{}/status", cfg.base),
                        QoS::AtLeastOnce,
                        true,
                        "online",
                    )
                    .await;
                session.fetch_add(1, Ordering::SeqCst);
            }
            Ok(Event::Incoming(Packet::Publish(p))) => {
                let payload = String::from_utf8_lossy(&p.payload).to_string();
                match parse_command(&p.topic, &payload, &cfg.base) {
                    Some(cmd) if app.read_only => {
                        tracing::warn!("mqtt: ignoring {cmd:?} (READ_ONLY)")
                    }
                    Some(cmd) => {
                        tracing::info!("mqtt: {} {payload:?} -> {cmd:?}", p.topic);
                        let a = app.clone();
                        let c = cmd.clone();
                        let room = match cmd {
                            Command::Hvac { id, .. }
                            | Command::Preset { id, .. }
                            | Command::Target { id, .. } => Some(id),
                            Command::House { .. } => None,
                        };
                        let said = match &cmd {
                            Command::Hvac { heat: false, .. } => "Modo Apagado".to_string(),
                            Command::Hvac { heat: true, .. } => "Encendida (Normal)".to_string(),
                            Command::Preset { mode, .. } => format!(
                                "Modo {}",
                                match *mode {
                                    "eco" => "Eco",
                                    "turbo" => "Confort",
                                    _ => "Normal",
                                }
                            ),
                            Command::Target { target, .. } => {
                                format!("Consigna → {target:.1} °C").replace('.', ",")
                            }
                            Command::House { mode } => {
                                format!("Toda la casa: modo {}", house_label(mode))
                            }
                        };
                        match tokio::task::spawn_blocking(move || execute(&a, &c)).await {
                            Ok(Ok(())) => {
                                app.log("ha", room, said);
                                crate::web::reread_later(&app, room, true)
                            }
                            Ok(Err(e)) => tracing::warn!("mqtt: {cmd:?} failed: {e:#}"),
                            Err(e) => tracing::warn!("mqtt: {cmd:?} panicked: {e}"),
                        }
                    }
                    None => tracing::debug!("mqtt: ignored {} {payload:?}", p.topic),
                }
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!("mqtt: {e}; retrying in 5 s");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Room;
    use caleon::device::RoomState;

    fn cfg() -> Config {
        Config::parse("mqtt://localhost").unwrap()
    }

    fn snapshot() -> Snapshot {
        let state = |mode: &str, tset: &str| {
            Some(RoomState {
                tset: tset.into(),
                mode: mode.into(),
                vacation: String::new(),
                season: "heating".into(),
                name: String::new(),
            })
        };
        Snapshot {
            device: "20f85e6e1ed1.sorel-connect.net".into(),
            firmware: Some("2023-12-07 33160".into()),
            rooms: vec![
                Room {
                    id: 0,
                    name: "Habitación 1".into(),
                    kind: "living".into(),
                    temperature: "22.5°C".into(),
                    humidity: "43.8%".into(),
                    state: state("regular", "21.5°C"),
                    outputs: vec![json!({"zone": "B", "label": "116.B Zona B", "on": true})],
                    week: None,
                },
                Room {
                    id: 1,
                    name: "Habitación 2".into(),
                    kind: "bedroom1".into(),
                    temperature: "19.9°C".into(),
                    humidity: "--".into(),
                    state: state("off", "Off"),
                    outputs: vec![],
                    week: None,
                },
            ],
            system: state("off", "Off"),
            status: vec![caleon::status::Metric {
                key: "Salidas · 116.A Bomba circuito".into(),
                group: "Salidas".into(),
                label: "116.A Bomba circuito".into(),
                text: "On".into(),
                value: Some(1.0),
                unit: "on/off".into(),
            }],
            updated: Some(1792137600),
            ..Default::default()
        }
    }

    #[test]
    fn parses_urls() {
        let c = Config::parse("mqtt://ha:p%40ss@homeassistant.local:1884").unwrap();
        assert_eq!(
            (
                c.host.as_str(),
                c.port,
                c.user.as_deref(),
                c.password.as_deref()
            ),
            ("homeassistant.local", 1884, Some("ha"), Some("p@ss"))
        );
        let c = Config::parse("mqtt://broker").unwrap();
        assert_eq!((c.port, c.user, c.base.as_str()), (1883, None, "caleon"));
        assert!(Config::parse("mqtts://x").is_err());
        assert!(Config::parse("mqtt://").is_err());
    }

    #[test]
    fn maps_modes() {
        assert_eq!(hvac("turbo 01:23"), ("heat", "comfort"));
        assert_eq!(hvac("eco"), ("heat", "eco"));
        assert_eq!(hvac("regular"), ("heat", "none"));
        assert_eq!(hvac("off"), ("off", "none"));
        assert_eq!(house_mode("confort"), Some("turbo"));
        assert_eq!(house_label("eco"), "Eco");
        assert_eq!(house_label(""), "Mixto");
        assert_eq!(house_mode("Mixto"), None);
    }

    #[test]
    fn discovery_messages() {
        let d = discovery(&snapshot(), &cfg());
        let topics: Vec<&str> = d.iter().map(|(t, _)| t.as_str()).collect();
        assert!(topics.contains(&"homeassistant/climate/caleon_20f85e6e1ed1/room0/config"));
        assert!(topics.contains(&"homeassistant/sensor/caleon_20f85e6e1ed1/room0_humidity/config"));
        assert!(!topics.contains(&"homeassistant/sensor/caleon_20f85e6e1ed1/room1_humidity/config"));
        assert!(topics.contains(&"homeassistant/binary_sensor/caleon_20f85e6e1ed1/pump/config"));
        assert!(!topics.iter().any(|t| t.contains("/boiler/")));
        assert!(topics.contains(&"homeassistant/select/caleon_20f85e6e1ed1/house/config"));
        let climate = &d[0].1;
        assert_eq!(
            climate["temperature_command_topic"],
            "caleon/room/0/set/temperature"
        );
        assert_eq!(climate["current_humidity_topic"], "caleon/room/0/humidity");
        assert_eq!(climate["unique_id"], "caleon_20f85e6e1ed1_room0");
        assert!(climate.get("object_id").is_none());
        assert_eq!(climate["device"]["name"], "Caleon");
        let house = &d.iter().find(|(t, _)| t.contains("/select/")).unwrap().1;
        assert_eq!(house["options"].as_array().unwrap().len(), 5);
        assert_eq!(climate["device"]["identifiers"][0], "caleon_20f85e6e1ed1");
        assert_eq!(climate["availability_topic"], "caleon/status");
    }

    #[test]
    fn state_messages() {
        let s = states(&snapshot(), &cfg());
        let get = |t: &str| {
            s.iter()
                .find(|(k, _)| k == t)
                .map(|(_, v)| v.as_str())
                .unwrap_or_else(|| panic!("{t}"))
        };
        assert_eq!(get("caleon/room/0/mode"), "heat");
        assert_eq!(get("caleon/room/0/preset"), "none");
        assert_eq!(get("caleon/room/0/temperature"), "22.5");
        assert_eq!(get("caleon/room/0/humidity"), "43.8");
        assert_eq!(get("caleon/room/0/target"), "21.5");
        assert_eq!(get("caleon/room/0/heating"), "ON");
        assert_eq!(get("caleon/room/0/action"), "heating");
        assert_eq!(get("caleon/room/1/mode"), "off");
        assert_eq!(get("caleon/room/1/target"), "None");
        assert_eq!(get("caleon/room/1/action"), "off");
        assert!(!s.iter().any(|(k, _)| k == "caleon/room/1/humidity"));
        assert_eq!(get("caleon/pump"), "ON");
        assert_eq!(get("caleon/house/mode"), "Apagado");
        assert_eq!(get("caleon/last_read"), "2026-10-16T08:00:00+00:00");
    }

    #[test]
    fn commands() {
        assert_eq!(
            parse_command("caleon/room/0/set/mode", "heat", "caleon"),
            Some(Command::Hvac { id: 0, heat: true })
        );
        assert_eq!(
            parse_command("caleon/room/3/set/mode", "off\n", "caleon"),
            Some(Command::Hvac { id: 3, heat: false })
        );
        assert_eq!(
            parse_command("caleon/room/1/set/preset", "comfort", "caleon"),
            Some(Command::Preset {
                id: 1,
                mode: "turbo"
            })
        );
        assert_eq!(
            parse_command("caleon/room/1/set/preset", "none", "caleon"),
            Some(Command::Preset {
                id: 1,
                mode: "regular"
            })
        );
        assert_eq!(
            parse_command("caleon/room/1/set/temperature", "21.5", "caleon"),
            Some(Command::Target {
                id: 1,
                target: 21.5
            })
        );
        assert_eq!(
            parse_command("caleon/house/set/mode", "Eco", "caleon"),
            Some(Command::House { mode: "eco" })
        );
        assert_eq!(
            parse_command("caleon/room/1/temperature", "21.5", "caleon"),
            None
        );
        assert_eq!(
            parse_command("other/room/1/set/mode", "heat", "caleon"),
            None
        );
        assert_eq!(
            parse_command("caleon/room/x/set/mode", "heat", "caleon"),
            None
        );
        assert_eq!(steps(21.5, 23.0), 3);
        assert_eq!(steps(21.5, 20.0), -3);
        assert_eq!(steps(21.5, 21.6), 0);
    }
}

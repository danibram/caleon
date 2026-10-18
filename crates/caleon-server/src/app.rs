//! Shared state, the poller, the menu crawler and configuration backups.

use crate::connection::Settings;
use crate::store::{RoomSample, Store};
use anyhow::{anyhow, bail, Result};
use caleon::device::{self, Device, RoomChange, RoomState, SYSTEM_ROOM};
use caleon::schema::Schema;
use caleon::status::{self, Metric};
use caleon::transport::Found;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use ts_rs::TS;

pub struct App {
    pub device: Device,
    pub store: Store,
    pub data_dir: PathBuf,
    pub snapshot: RwLock<Snapshot>,
    /// Set while a crawl walks the device menu, which moves the device's menu cursor.
    pub crawling: AtomicBool,
    /// Screens read so far by the running crawl, and how many the last one read.
    pub crawl_done: AtomicUsize,
    pub crawl_expected: AtomicUsize,
    /// Hours between automatic backups (0 = disabled).
    pub backup_hours: u64,
    /// READ_ONLY=1: refuse every write to the thermostat (for reviews and demos).
    pub read_only: bool,
    /// PROMETHEUS=0 turns off the /metrics exposition.
    pub prometheus: bool,
    /// WEB=0 serves only the API and /metrics, not the web UI.
    pub web: bool,
    pub schema: Arc<Schema>,
    /// Current connection settings and where the thermostat was found.
    pub conn: RwLock<(Settings, Found)>,
}

#[derive(Debug, Clone, Serialize, Default, TS)]
pub struct Snapshot {
    pub device: String,
    pub firmware: Option<String>,
    pub nabto: Option<String>,
    #[ts(type = "{ [key: string]: boolean }")]
    pub features: Map<String, Value>,
    pub rooms: Vec<Room>,
    pub system: Option<RoomState>,
    pub status: Vec<Metric>,
    pub status_lines: Vec<Vec<String>>,
    /// Zone letter -> room type, read from Ajustes > Zonas during a crawl.
    #[ts(type = "{ [key: string]: string }")]
    pub zones: Map<String, Value>,
    /// Sensor label ("1", "116.2") -> device room name, from Habitaciones > <room> > Temperatura.
    #[ts(type = "{ [key: string]: string }")]
    pub sensors: Map<String, Value>,
    /// CAN id -> device name ("116" -> "°CALEONbox"), from the last status screen.
    #[ts(type = "{ [key: string]: string }")]
    pub devices: Map<String, Value>,
    #[ts(type = "number | null")]
    pub updated: Option<i64>,
    pub error: Option<String>,
    #[serde(skip)]
    #[ts(skip)]
    pub weeks_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct Room {
    pub id: u16,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub temperature: String,
    pub humidity: String,
    /// ts-rs cannot flatten an Option, so web/src/api/types.ts adds the RoomState fields.
    #[serde(flatten)]
    #[ts(skip)]
    pub state: Option<RoomState>,
    /// Zone outputs that heat this room, with their current state.
    #[ts(type = "Array<{ label: string, on: boolean, zone: string }>")]
    pub outputs: Vec<Value>,
    /// Weekly program of the active season (7 days x 48 half hours, Monday first).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub week: Option<Vec<Vec<String>>>,
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

pub fn parse_number(s: &str) -> Option<f64> {
    let start = s.find(|c: char| c.is_ascii_digit() || c == '-')?;
    let rest = &s[start..];
    let end = rest
        .char_indices()
        .skip(1)
        .find(|(_, c)| !(c.is_ascii_digit() || *c == '.' || *c == ','))
        .map_or(rest.len(), |(i, _)| i);
    rest[..end].replace(',', ".").parse().ok()
}

/// Titles that must never be entered automatically.
fn unsafe_title(title: &str) -> bool {
    let t = title.to_lowercase();
    [
        "fábrica",
        "fabrica",
        "factory",
        "werks",
        "reset",
        "borrar",
        "eliminar",
        "restabl",
        "actualiz",
        "update",
        "test",
        "prueba",
        "manual",
        "seleccionar red",
        "select network",
        "añadir",
        "anadir",
        "add ",
    ]
    .iter()
    .any(|w| t.contains(w))
}

impl App {
    pub fn extended(&self) -> bool {
        self.snapshot
            .read()
            .unwrap()
            .features
            .get("extended_api")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    pub fn has(&self, feature: &str) -> bool {
        self.snapshot
            .read()
            .unwrap()
            .features
            .get(feature)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// Reads everything shown on the overview and the status screens. Blocking.
    pub fn refresh(&self, record: bool) -> Result<()> {
        let d = &self.device;
        let features = d.features()?;
        let list = d.room_list()?;
        let mut rooms = Vec::new();
        for (i, r) in list.iter().enumerate() {
            let kind = r["type"].as_str().unwrap_or("").to_string();
            let state = if kind == "outdoor" {
                None
            } else {
                Some(d.room_state(i as u16, &RoomChange::default())?)
            };
            rooms.push(Room {
                id: i as u16,
                name: r["name"].as_str().unwrap_or("").to_string(),
                kind,
                temperature: r["temperature"].as_str().unwrap_or("").to_string(),
                humidity: r["humidity"].as_str().unwrap_or("").to_string(),
                state,
                outputs: vec![],
                week: None,
            });
        }
        let system_control = features
            .get("system_control")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let system = if system_control {
            Some(d.room_state(SYSTEM_ROOM, &RoomChange::default())?)
        } else {
            None
        };

        // status screens: cal:9, cal:9.1, ... until an empty page
        let extended = features
            .get("extended_api")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let mut pages = Vec::new();
        for n in 0..6 {
            let id = if n == 0 {
                "cal:9".to_string()
            } else {
                format!("cal:9.{n}")
            };
            let lines: Vec<String> = match d.menu_detail(extended, "information", &id) {
                Ok(v) => v["menu"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|l| l["text"].as_str().unwrap_or("").to_string())
                            .collect()
                    })
                    .unwrap_or_default(),
                Err(e) => {
                    tracing::debug!("status page {id}: {e}");
                    vec![]
                }
            };
            if lines.is_empty() {
                break;
            }
            pages.push(lines);
        }
        let metrics = status::parse(&pages.concat());

        let ts = now();
        // weekly programs change rarely, so reread them every 10 minutes
        let (prev_weeks, weeks_at) = {
            let snap = self.snapshot.read().unwrap();
            (
                snap.rooms
                    .iter()
                    .map(|r| (r.id, r.week.clone()))
                    .collect::<Vec<_>>(),
                snap.weeks_at,
            )
        };
        let reread = ts - weeks_at.unwrap_or(0) > 600;
        for room in rooms.iter_mut() {
            let Some(state) = &room.state else { continue };
            room.week = if reread {
                let t = if state.season == "cooling" { 1 } else { 0 };
                d.room_timer(room.id, t, None)
                    .ok()
                    .map(|v| device::decode_week(&v, &device::ROOM_MODES))
            } else {
                prev_weeks
                    .iter()
                    .find(|(id, _)| *id == room.id)
                    .and_then(|(_, w)| w.clone())
            };
        }
        let mut snap = self.snapshot.write().unwrap();
        if reread {
            snap.weeks_at = Some(ts);
        }
        if snap.firmware.is_none()
            && features
                .get("versions")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        {
            if let Ok(v) = d.versions() {
                snap.firmware = v["firmware"].as_str().map(String::from);
                snap.nabto = v["nabto"].as_str().map(String::from);
            }
        }
        // link "116.B Zona B: On" outputs to rooms through the zone mapping
        for room in rooms.iter_mut() {
            room.outputs = snap
                .zones
                .iter()
                .filter(|(_, kind)| kind.as_str() == Some(room.kind.as_str()))
                .filter_map(|(zone, _)| {
                    metrics
                        .iter()
                        .find(|m| {
                            m.group == "Salidas" && m.label.ends_with(&format!("Zona {zone}"))
                        })
                        .map(
                            |m| json!({"zone": zone, "label": m.label, "on": m.value == Some(1.0)}),
                        )
                })
                .collect();
        }
        if record {
            let samples: Vec<RoomSample> = rooms
                .iter()
                .map(|r| RoomSample {
                    room: r.id,
                    temperature: parse_number(&r.temperature),
                    humidity: parse_number(&r.humidity),
                    tset: r.state.as_ref().and_then(|s| parse_number(&s.tset)),
                    mode: r.state.as_ref().map(|s| s.mode.clone()).unwrap_or_default(),
                })
                .collect();
            self.store.add_rooms(ts, &samples)?;
            let numeric: Vec<(String, f64)> = metrics
                .iter()
                .filter_map(|m| m.value.map(|v| (m.key.clone(), v)))
                .collect();
            self.store.add_metrics(ts, &numeric)?;
        }
        snap.features = features;
        snap.rooms = rooms;
        snap.system = system;
        snap.status = metrics;
        snap.devices = status::devices(&pages.concat())
            .into_iter()
            .map(|(id, name)| (id, json!(name)))
            .collect();
        snap.status_lines = pages;
        snap.updated = Some(ts);
        snap.error = None;
        Ok(())
    }

    pub fn apply_room_state(&self, id: u16, state: RoomState) {
        let mut snap = self.snapshot.write().unwrap();
        if id == SYSTEM_ROOM {
            snap.system = Some(state);
        } else if let Some(r) = snap.rooms.iter_mut().find(|r| r.id == id) {
            r.name = state.name.clone();
            r.state = Some(state);
        }
    }

    /// Changes the setpoint of one or more modes without leaving the room in another mode: the
    /// room visits each mode, moves that mode's setpoint in half-degree steps and returns to the
    /// mode it had. With `read_all` it also visits the modes not in `targets`, to learn their
    /// setpoints. Blocking. Returns the setpoints seen.
    pub fn set_mode_temps(
        &self,
        id: u16,
        targets: &[(String, f64)],
        read_all: bool,
    ) -> Result<Map<String, Value>> {
        if id == SYSTEM_ROOM {
            bail!("las temperaturas de los modos se cambian habitación por habitación");
        }
        let cur = self.device.room_state(id, &RoomChange::default())?;
        if !cur.vacation.is_empty() {
            bail!("la habitación está en vacaciones; termínalas antes");
        }
        let orig = match cur.mode.split(' ').next().unwrap_or("") {
            "" => "regular".to_string(),
            m => m.to_string(),
        };
        let mut modes: Vec<(String, Option<f64>)> =
            targets.iter().map(|(m, t)| (m.clone(), Some(*t))).collect();
        if read_all {
            for m in ["regular", "eco", "turbo"] {
                if !modes.iter().any(|(k, _)| k == m) {
                    modes.push((m.to_string(), None));
                }
            }
        }
        let mut seen = Map::new();
        let result = (|| -> Result<()> {
            for (mode, target) in &modes {
                let mut st = self.device.room_state(
                    id,
                    &RoomChange {
                        mode: Some(mode.clone()),
                        ..Default::default()
                    },
                )?;
                let mut now = parse_number(&st.tset)
                    .ok_or_else(|| anyhow!("el modo {mode} no devuelve consigna"))?;
                if let Some(target) = target {
                    // one relative step at a time, reading the answer back: a lost packet cannot overshoot
                    for _ in 0..60 {
                        let n = ((target - now) * 2.0).round() as i32;
                        if n == 0 {
                            break;
                        }
                        st = self.device.room_state(
                            id,
                            &RoomChange {
                                tdelta: Some(n.signum() as i8),
                                ..Default::default()
                            },
                        )?;
                        now = parse_number(&st.tset)
                            .ok_or_else(|| anyhow!("respuesta sin consigna"))?;
                    }
                }
                seen.insert(mode.clone(), json!(now));
                self.store.set_mode_temp(id, mode, now);
            }
            Ok(())
        })();
        // back to the mode the room had, whatever happened in between
        match self.device.room_state(
            id,
            &RoomChange {
                mode: Some(orig.clone()),
                ..Default::default()
            },
        ) {
            Ok(st) => self.apply_room_state(id, st),
            Err(e) => tracing::warn!("room {id}: could not return to {orig}: {e}"),
        }
        result?;
        Ok(seen)
    }

    // ---- crawler

    /// Walks the device menu read-only. It only enters "goto:" entries of string lists and dynamic
    /// lists that open sub-menus, never presses yes/no, +/- or selections, and skips entries
    /// such as factory reset or firmware update. Blocking.
    pub fn crawl_menu(&self) -> Result<Value> {
        if self.crawling.swap(true, Ordering::SeqCst) {
            bail!("ya hay un recorrido del menú en curso");
        }
        let result = (|| {
            let ext = self.extended();
            self.crawl_done.store(0, Ordering::SeqCst);
            let expected = self
                .list_backups()
                .ok()
                .and_then(|l| l.first().and_then(|b| b["name"].as_str().map(String::from)))
                .and_then(|n| self.read_backup(&n).ok())
                .map(|b| count_nodes(&b["menu"]))
                .unwrap_or(0);
            self.crawl_expected.store(expected, Ordering::SeqCst);
            let root = self.retry(|| self.device.menu_action(ext, "", ""))?;
            let mut count = 0;
            let tree = self.crawl_node(ext, &root, vec![], 0, &mut count);
            let _ = self
                .device
                .menu_action(ext, root["id"].as_str().unwrap_or(""), "exit");
            tree
        })();
        self.crawling.store(false, Ordering::SeqCst);
        let tree = result?;
        let zones = zone_map(&tree);
        let sensors = sensor_map(&tree);
        let mut snap = self.snapshot.write().unwrap();
        if !zones.is_empty() {
            snap.zones = zones;
        }
        if !sensors.is_empty() {
            snap.sensors = sensors;
        }
        drop(snap);
        Ok(tree)
    }

    /// Retries a device call a few times, because the device occasionally drops a UDP answer.
    fn retry<T>(&self, mut f: impl FnMut() -> Result<T>) -> Result<T> {
        let mut last = None;
        for attempt in 0..4 {
            match f() {
                Ok(v) => return Ok(v),
                Err(e) => {
                    tracing::debug!("crawl retry {attempt}: {e}");
                    last = Some(e);
                    std::thread::sleep(std::time::Duration::from_millis(400 * (attempt + 1)));
                }
            }
        }
        Err(last.unwrap())
    }

    fn crawl_node(
        &self,
        ext: bool,
        menu: &Value,
        path: Vec<String>,
        depth: usize,
        count: &mut usize,
    ) -> Result<Value> {
        *count += 1;
        self.crawl_done.fetch_add(1, Ordering::SeqCst);
        let id = menu["id"].as_str().unwrap_or("").to_string();
        let kind = menu["type"].as_str().unwrap_or("").to_string();
        let title = menu["title"].as_str().unwrap_or("").to_string();
        let detail = self
            .device
            .menu_detail(ext, &kind, &id)
            .unwrap_or_else(|e| json!({"error": e.to_string()}));
        let mut children = Vec::new();
        // dynamic lists that only hold "goto:" entries are sub-menus (e.g. Gestión de dispositivos)
        let is_submenu_list = kind == "string list"
            || (kind.starts_with("dynamic list")
                && path.last().map(String::as_str) == Some("Habitaciones")
                && detail["menu"].as_array().is_some_and(|a| {
                    !a.is_empty()
                        && a.iter().all(|i| {
                            i["action"]
                                .as_str()
                                .is_none_or(|x| x.is_empty() || x.starts_with("goto:"))
                        })
                }));
        if is_submenu_list && depth < 8 && *count < 500 {
            for item in detail["menu"].as_array().cloned().unwrap_or_default() {
                let action = item["action"].as_str().unwrap_or("");
                let item_title = item["title"].as_str().unwrap_or("").to_string();
                if !action.starts_with("goto:")
                    || item_title.is_empty()
                    || unsafe_title(&item_title)
                {
                    continue;
                }
                // opening a screen is absolute (goto:N), so a lost answer can be retried from the parent
                let child = match self.retry(|| {
                    let r = self.device.menu_action(ext, &id, action);
                    if r.is_err() {
                        let _ = self.renavigate(ext, &path);
                    }
                    r
                }) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("crawl: cannot open {item_title}: {e}");
                        self.retry(|| self.renavigate(ext, &path))?;
                        continue;
                    }
                };
                let mut p = path.clone();
                p.push(item_title);
                children.push(self.crawl_node(ext, &child, p, depth + 1, count)?);
                let back = child["back"].as_str().unwrap_or("");
                let child_id = child["id"].as_str().unwrap_or("");
                let landed = if !back.is_empty() && back != "exit" {
                    self.device.menu_action(ext, child_id, back)
                } else {
                    self.device.menu_action(ext, "", "")
                };
                if landed.as_ref().ok().and_then(|l| l["id"].as_str()) != Some(id.as_str()) {
                    // lost answer or unexpected screen, so get back on track through the root
                    tracing::warn!(
                        "crawl: back from {child_id} did not land on {id}, re-navigating"
                    );
                    self.retry(|| self.renavigate(ext, &path))?;
                }
            }
        }
        Ok(
            json!({"id": id, "title": title, "type": kind, "path": path, "menu": menu, "detail": detail, "children": children}),
        )
    }

    /// Opens the menu at `path` (list of titles) starting from the root. Only follows "goto:"
    /// entries, so it never changes a setting.
    pub fn renavigate(&self, ext: bool, path: &[String]) -> Result<Value> {
        let mut cur = self.device.menu_action(ext, "", "")?;
        for title in path {
            let id = cur["id"].as_str().unwrap_or("").to_string();
            let kind = cur["type"].as_str().unwrap_or("string list").to_string();
            let list = self.device.menu_detail(ext, &kind, &id)?;
            let action = list["menu"]
                .as_array()
                .and_then(|a| a.iter().find(|i| i["title"].as_str() == Some(title)))
                .and_then(|i| i["action"].as_str())
                .ok_or_else(|| anyhow!("menu entry {title} not found"))?
                .to_string();
            // only open screens, because a title can also name a selection or a toggle, which would write
            if !action.starts_with("goto:") {
                bail!("menu entry {title} is not a screen");
            }
            cur = self.device.menu_action(ext, &id, &action)?;
        }
        Ok(cur)
    }

    // ---- backups

    pub fn backups_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    /// Full configuration snapshot: rooms, weekly timers and every menu screen. Blocking.
    /// With `only_if_changed`, nothing is written when the configuration matches the newest
    /// backup (returns None), so daily backups don't pile up identical files.
    pub fn create_backup(&self, reason: &str, only_if_changed: bool) -> Result<Option<String>> {
        self.refresh(false)?;
        let snap = self.snapshot.read().unwrap().clone();
        let mut rooms = Vec::new();
        for r in &snap.rooms {
            let mut timers = Map::new();
            for (t, name) in [(0u8, "heating"), (1, "cooling")] {
                match self.device.room_timer(r.id, t, None) {
                    Ok(v) => {
                        timers.insert(
                            name.into(),
                            json!(device::decode_week(&v, &device::ROOM_MODES)),
                        );
                    }
                    Err(e) => tracing::warn!("backup: timer {name} of room {}: {e}", r.id),
                }
            }
            rooms.push(json!({"id": r.id, "name": r.name, "type": r.kind, "state": r.state, "timers": timers}));
        }
        let hws = if self.has("dhw_timer") {
            self.device
                .room_timer(SYSTEM_ROOM, 2, None)
                .ok()
                .map(|v| device::decode_week(&v, &device::HWS_MODES))
        } else {
            None
        };
        let menu = self.crawl_menu()?;
        let created = now();
        let backup = json!({
            "format": "caleon-backup/1",
            "created": created,
            "reason": reason,
            "device": snap.device,
            "firmware": snap.firmware,
            "features": snap.features,
            "rooms": rooms,
            "system": snap.system,
            "hws_timer": hws,
            "zones": self.snapshot.read().unwrap().zones,
            "sensors": self.snapshot.read().unwrap().sensors,
            "menu": menu,
        });
        if only_if_changed {
            let newest = self
                .list_backups()?
                .first()
                .and_then(|b| b["name"].as_str().map(String::from));
            if let Some(prev) = newest.and_then(|n| self.read_backup(&n).ok()) {
                if config_fingerprint(&prev) == config_fingerprint(&backup) {
                    tracing::info!("backup skipped ({reason}): no changes since the last one");
                    return Ok(None);
                }
            }
        }
        std::fs::create_dir_all(self.backups_dir())?;
        let name = format!("caleon-{}.json", timestamp_name(created));
        std::fs::write(
            self.backups_dir().join(&name),
            serde_json::to_vec_pretty(&backup)?,
        )?;
        tracing::info!("backup {name} written ({reason})");
        Ok(Some(name))
    }

    pub fn list_backups(&self) -> Result<Vec<Value>> {
        let mut out = Vec::new();
        if let Ok(entries) = std::fs::read_dir(self.backups_dir()) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if !name.ends_with(".json") {
                    continue;
                }
                let meta = e.metadata()?;
                let head: Value = std::fs::read(e.path())
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .unwrap_or(Value::Null);
                out.push(json!({
                    "name": name, "size": meta.len(), "created": head["created"], "reason": head["reason"],
                    "rooms": head["rooms"].as_array().map(|r| r.len()), "firmware": head["firmware"],
                }));
            }
        }
        out.sort_by(|a, b| b["created"].as_i64().cmp(&a["created"].as_i64()));
        Ok(out)
    }

    pub fn read_backup(&self, name: &str) -> Result<Value> {
        if name.contains('/') || name.contains("..") || !name.ends_with(".json") {
            bail!("invalid backup name");
        }
        Ok(serde_json::from_slice(&std::fs::read(
            self.backups_dir().join(name),
        )?)?)
    }

    /// Deletes old automatic backups, keeping the newest `keep`.
    pub fn prune_backups(&self, keep: usize) {
        if let Ok(list) = self.list_backups() {
            for b in list
                .iter()
                .filter(|b| b["reason"] == "automática")
                .skip(keep)
            {
                if let Some(n) = b["name"].as_str() {
                    let _ = std::fs::remove_file(self.backups_dir().join(n));
                }
            }
        }
    }

    /// Compares a backup with the device. Restores room names and weekly timers when `apply`;
    /// menu values are only compared (they can only be changed step by step with +/-).
    pub fn restore(&self, backup: &Value, apply: bool, names: bool, timers: bool) -> Result<Value> {
        let mut changes = Vec::new();
        let current = self.snapshot.read().unwrap().clone();
        for r in backup["rooms"].as_array().cloned().unwrap_or_default() {
            let id = r["id"].as_u64().ok_or_else(|| anyhow!("room without id"))? as u16;
            let Some(cur) = current.rooms.iter().find(|c| c.id == id) else {
                changes.push(json!({"what": format!("Habitación {id}"), "note": "ya no existe en el equipo"}));
                continue;
            };
            let name = r["name"].as_str().unwrap_or("");
            if names && !name.is_empty() && name != cur.name {
                changes.push(json!({"what": format!("Nombre de {}", cur.name), "from": cur.name, "to": name}));
                if apply {
                    let st = self.device.room_state(
                        id,
                        &RoomChange {
                            name: Some(name.into()),
                            ..Default::default()
                        },
                    )?;
                    self.apply_room_state(id, st);
                }
            }
            if !timers {
                continue;
            }
            for (t, kind) in [(0u8, "heating"), (1, "cooling")] {
                let Some(week) = r["timers"][kind].as_array() else {
                    continue;
                };
                let now_week =
                    device::decode_week(&self.device.room_timer(id, t, None)?, &device::ROOM_MODES);
                for (day, slots) in week.iter().enumerate() {
                    let slots: Vec<String> = serde_json::from_value(slots.clone())?;
                    if now_week.get(day) != Some(&slots) {
                        changes.push(json!({"what": format!("Programa {} de {}, {}", if t == 0 { "calefacción" } else { "refrigeración" }, cur.name, DAYS[day]), "from": now_week.get(day), "to": slots}));
                        if apply {
                            let bf = device::bitfield(&slots, &device::ROOM_MODES)?;
                            self.device.room_timer(id, t, Some((day as u8, bf)))?;
                        }
                    }
                }
            }
        }
        // menu values: compare by path against a fresh crawl
        let mut menu_diffs = Vec::new();
        if !backup["menu"].is_null() {
            let now_menu = self.crawl_menu()?;
            let mut a = Vec::new();
            let mut b = Vec::new();
            flatten_values(&backup["menu"], &mut a);
            flatten_values(&now_menu, &mut b);
            for (path, val) in &a {
                if let Some((_, cur)) = b.iter().find(|(p, _)| p == path) {
                    if cur != val {
                        menu_diffs.push(json!({"path": path, "backup": val, "current": cur}));
                    }
                }
            }
        }
        Ok(json!({"applied": apply, "changes": changes, "menu_differences": menu_diffs}))
    }
}

pub const DAYS: [&str; 7] = [
    "lunes",
    "martes",
    "miércoles",
    "jueves",
    "viernes",
    "sábado",
    "domingo",
];

/// (path, shown value) for every value-bearing screen of a crawled menu tree.
/// What a backup configures, without the parts that change on their own: when it was made, live
/// sensor readings and the menu tree's shape (only each screen's value counts).
fn config_fingerprint(b: &Value) -> Value {
    let mut values = Vec::new();
    flatten_values(&b["menu"], &mut values);
    json!({
        "rooms": b["rooms"], "system": b["system"], "hws_timer": b["hws_timer"],
        "zones": b["zones"], "sensors": b["sensors"], "firmware": b["firmware"], "values": values,
    })
}

pub fn flatten_values(node: &Value, out: &mut Vec<(String, String)>) {
    let path = node["path"]
        .as_array()
        .map(|p| {
            p.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" › ")
        })
        .unwrap_or_default();
    let d = &node["detail"];
    let value = match node["type"].as_str().unwrap_or("") {
        "options" | "binary query" | "text" => d["value"].as_str().map(String::from),
        "dynamic list" | "dynamic list tiled" => {
            let sel: Vec<String> = d["menu"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter(|i| i["highlighted"].as_u64().unwrap_or(0) != 0)
                        .map(|i| {
                            // sensor pickers show live readings in the title ("1-Wire Sensor:25.2°C ..."),
                            // so the configured choice is the sensor id, not its current value
                            let title = i["title"].as_str().unwrap_or("");
                            let live =
                                title.contains(':') && title.chars().any(|c| c.is_ascii_digit());
                            let icon_text = i["icon_text"].as_str().unwrap_or("");
                            let shown = if live || title.is_empty() {
                                icon_text
                            } else {
                                title
                            };
                            let fallback = if shown.is_empty() {
                                i["icon"].as_str().unwrap_or("")
                            } else {
                                shown
                            };
                            if live || title.is_empty() {
                                fallback.to_string()
                            } else {
                                format!("{icon_text} {title}").trim().to_string()
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            if sel.is_empty() {
                None
            } else {
                Some(sel.join(", "))
            }
        }
        // the one information screen that is configuration: e-mails allowed to connect
        "information" if path.ends_with("Control de acceso") => d["menu"].as_array().map(|a| {
            a.iter()
                .filter_map(|l| l["text"].as_str())
                .filter(|t| !t.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        }),
        "offset" => d["menu"].as_array().map(|a| {
            a.iter()
                .map(|i| {
                    format!(
                        "{}={}",
                        i["icon_text"].as_str().unwrap_or(""),
                        i["offset"].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        }),
        _ => None,
    };
    if let Some(v) = value {
        out.push((path, v));
    }
    for c in node["children"].as_array().into_iter().flatten() {
        flatten_values(c, out);
    }
}

fn count_nodes(n: &Value) -> usize {
    1 + n["children"]
        .as_array()
        .map_or(0, |c| c.iter().map(count_nodes).sum())
}

/// Sensor label -> room name, from Ajustes > Habitaciones > <room> > Temperatura (highlighted sensor).
fn sensor_map(tree: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    fn walk(n: &Value, out: &mut Map<String, Value>) {
        let path: Vec<&str> = n["path"]
            .as_array()
            .map(|p| p.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if path.len() >= 3
            && path[path.len() - 1] == "Temperatura"
            && path[path.len() - 3] == "Habitaciones"
        {
            for item in n["detail"]["menu"].as_array().into_iter().flatten() {
                if item["highlighted"].as_u64().unwrap_or(0) != 0 {
                    if let Some(label) = item["icon_text"].as_str().filter(|l| !l.is_empty()) {
                        out.insert(label.to_string(), json!(path[path.len() - 2]));
                    }
                }
            }
        }
        for c in n["children"].as_array().into_iter().flatten() {
            walk(c, out);
        }
    }
    walk(tree, &mut out);
    out
}

/// Zone letter -> room type, from Ajustes > Zonas > Zona X > Habitación (highlighted icon).
fn zone_map(tree: &Value) -> Map<String, Value> {
    let mut out = Map::new();
    fn walk(n: &Value, out: &mut Map<String, Value>) {
        let path: Vec<&str> = n["path"]
            .as_array()
            .map(|p| p.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if path.len() >= 2
            && path[path.len() - 1] == "Habitación"
            && path[path.len() - 2].starts_with("Zona ")
        {
            let zone = path[path.len() - 2].trim_start_matches("Zona ").to_string();
            if let Some(item) = n["detail"]["menu"].as_array().and_then(|a| {
                a.iter()
                    .find(|i| i["highlighted"].as_u64().unwrap_or(0) != 0)
            }) {
                out.insert(zone, item["icon"].clone());
            }
        }
        for c in n["children"].as_array().into_iter().flatten() {
            walk(c, out);
        }
    }
    walk(tree, &mut out);
    out
}

/// UTC "YYYYmmdd-HHMMSS" without a date library.
pub fn timestamp_name(ts: i64) -> String {
    let days = ts.div_euclid(86400);
    let secs = ts.rem_euclid(86400);
    // civil_from_days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp() {
        assert_eq!(timestamp_name(1792094926), "20261015-200846");
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_number("25.2°C"), Some(25.2));
        assert_eq!(parse_number("72.8%"), Some(72.8));
        assert_eq!(parse_number("Off"), None);
    }
}

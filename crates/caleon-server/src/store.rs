//! SQLite history: room samples and every numeric value shown on the device status screens.

use anyhow::Result;
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

pub struct Store {
    db: Mutex<Connection>,
    /// Last setpoint seen per (room, mode). Kept in memory because /api/state asks for it on every
    /// call and computing it from the samples table means a full scan.
    mode_temps: Mutex<BTreeMap<(u16, String), f64>>,
}

/// "turbo 01:23" -> "turbo"
fn base_mode(mode: &str) -> &str {
    mode.split(' ').next().unwrap_or("")
}

pub struct RoomSample {
    pub room: u16,
    pub temperature: Option<f64>,
    pub humidity: Option<f64>,
    pub tset: Option<f64>,
    pub mode: String,
}

impl Store {
    pub fn open(path: &Path) -> Result<Store> {
        let db = Connection::open(path)?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS samples (
               ts INTEGER NOT NULL, room INTEGER NOT NULL, temperature REAL, humidity REAL,
               tset REAL, mode TEXT, PRIMARY KEY (ts, room));
             CREATE TABLE IF NOT EXISTS metrics (
               ts INTEGER NOT NULL, key TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY (ts, key));
             CREATE INDEX IF NOT EXISTS metrics_key_ts ON metrics(key, ts);
             CREATE TABLE IF NOT EXISTS mode_setpoints (
               room INTEGER NOT NULL, mode TEXT NOT NULL, tset REAL NOT NULL, ts INTEGER NOT NULL,
               PRIMARY KEY (room, mode));",
        )?;
        let mut temps = BTreeMap::new();
        {
            let mut stmt = db.prepare(
                "SELECT room, mode, tset FROM samples s WHERE tset IS NOT NULL AND mode NOT IN ('', 'off')
                 AND ts = (SELECT MAX(ts) FROM samples t WHERE t.room = s.room AND t.mode = s.mode AND t.tset IS NOT NULL)",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, f64>(2)?,
                ))
            })?;
            for row in rows {
                let (room, mode, tset) = row?;
                temps.insert((room as u16, base_mode(&mode).to_string()), tset);
            }
        }
        {
            // setpoints read on purpose (App::set_mode_temps) win over what the samples suggest
            let mut stmt = db.prepare("SELECT room, mode, tset FROM mode_setpoints")?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, f64>(2)?,
                ))
            })?;
            for row in rows {
                let (room, mode, tset) = row?;
                temps.insert((room as u16, mode), tset);
            }
        }
        Ok(Store {
            db: Mutex::new(db),
            mode_temps: Mutex::new(temps),
        })
    }

    pub fn add_rooms(&self, ts: i64, rooms: &[RoomSample]) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        for r in rooms {
            tx.execute(
                "INSERT OR REPLACE INTO samples VALUES (?1,?2,?3,?4,?5,?6)",
                params![ts, r.room, r.temperature, r.humidity, r.tset, r.mode],
            )?;
        }
        tx.commit()?;
        drop(db);
        let mut temps = self.mode_temps.lock().unwrap();
        for r in rooms {
            let mode = base_mode(&r.mode);
            if let (Some(t), false) = (r.tset, mode.is_empty() || mode == "off") {
                temps.insert((r.room, mode.to_string()), t);
            }
        }
        Ok(())
    }

    pub fn add_metrics(&self, ts: i64, metrics: &[(String, f64)]) -> Result<()> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        for (k, v) in metrics {
            tx.execute(
                "INSERT OR REPLACE INTO metrics VALUES (?1,?2,?3)",
                params![ts, k, v],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Setpoint of a mode read by visiting it; kept across restarts.
    pub fn set_mode_temp(&self, room: u16, mode: &str, tset: f64) {
        self.mode_temps
            .lock()
            .unwrap()
            .insert((room, mode.to_string()), tset);
        let db = self.db.lock().unwrap();
        if let Err(e) = db.execute(
            "INSERT OR REPLACE INTO mode_setpoints VALUES (?1,?2,?3,?4)",
            params![room, mode, tset, crate::app::now()],
        ) {
            tracing::warn!("mode_setpoints: {e}");
        }
    }

    /// Last setpoint seen for each room and mode: {"0": {"regular": 21.5, "turbo": 23.5}}.
    pub fn mode_temps(&self) -> Value {
        let mut out = Map::new();
        for ((room, mode), t) in self.mode_temps.lock().unwrap().iter() {
            out.entry(room.to_string())
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .unwrap()
                .insert(mode.clone(), json!(t));
        }
        Value::Object(out)
    }

    /// Timestamp of the oldest room sample, if any.
    pub fn history_since(&self) -> Option<i64> {
        let db = self.db.lock().unwrap();
        db.query_row("SELECT MIN(ts) FROM samples", [], |r| {
            r.get::<_, Option<i64>>(0)
        })
        .ok()
        .flatten()
    }

    /// {room: [[ts, temperature, humidity, tset, mode], ...]}
    pub fn room_history(&self, since: i64) -> Result<Value> {
        let db = self.db.lock().unwrap();
        let mut stmt =
            db.prepare("SELECT ts, room, temperature, humidity, tset, mode FROM samples WHERE ts >= ?1 ORDER BY ts")?;
        let mut out = Map::new();
        let rows = stmt.query_map([since], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<f64>>(2)?,
                r.get::<_, Option<f64>>(3)?,
                r.get::<_, Option<f64>>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })?;
        for row in rows {
            let (ts, room, t, h, tset, mode) = row?;
            out.entry(room.to_string())
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .unwrap()
                .push(json!([ts, t, h, tset, mode]));
        }
        Ok(Value::Object(out))
    }

    /// {key: [[ts, value], ...]}, downsampled to at most ~`points` per key.
    pub fn metric_history(&self, since: i64, points: i64) -> Result<Value> {
        let db = self.db.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs() as i64;
        let bucket = ((now - since) / points.max(1)).max(1);
        let mut stmt = db.prepare(
            "SELECT key, (ts / ?2) * ?2 AS b, AVG(value) FROM metrics WHERE ts >= ?1 GROUP BY key, b ORDER BY key, b",
        )?;
        let mut out = Map::new();
        let rows = stmt.query_map(params![since, bucket], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, f64>(2)?,
            ))
        })?;
        for row in rows {
            let (k, ts, v) = row?;
            out.entry(k)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .unwrap()
                .push(json!([ts, v]));
        }
        Ok(Value::Object(out))
    }
}

//! °CALEON RPC protocol, mirroring exactly what the official app (com.sorel.caleon 7.0.36) sends.

use crate::schema::Arg;
use crate::transport::Transport;
use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

pub const SYSTEM_ROOM: u16 = 65535;
pub const ROOM_MODES: [&str; 4] = ["off", "regular", "eco", "turbo"];
pub const HWS_MODES: [&str; 2] = ["hws_off", "hws_on"];
pub const SERVICE_TIMER_MODES: [&str; 2] = ["off", "on"];

/// synch_flags.json bit positions, from the official app.
const FEATURES: [(&str, u32); 8] = [
    ("system_control", 0),
    ("dhw_timer", 1),
    ("season_menu", 2),
    ("versions", 3),
    ("message_log", 4),
    ("room_renaming", 5),
    ("pro_upgrade", 6),
    ("extended_api", 7),
];

pub struct Device {
    pub id: String,
    transport: RwLock<Arc<dyn Transport>>,
    /// Calls made since start, and when the ones of the last minute happened.
    calls: AtomicU64,
    recent: Mutex<VecDeque<Instant>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct RoomState {
    pub tset: String,
    pub mode: String,
    pub vacation: String,
    pub season: String,
    pub name: String,
}

fn int(n: impl Into<i64>) -> Arg {
    Arg::Int(n.into())
}

/// Raw strings go NUL-terminated, as the app sends them; an empty one means "leave unchanged".
fn raw(s: &str) -> Arg {
    if s.is_empty() {
        Arg::Bytes(vec![])
    } else {
        Arg::Bytes(format!("{s}\0").into_bytes())
    }
}

fn raw_opt(v: &Option<String>) -> Arg {
    raw(v.as_deref().unwrap_or(""))
}

impl Device {
    pub fn new(id: String, transport: Arc<dyn Transport>) -> Device {
        Device {
            id,
            transport: RwLock::new(transport),
            calls: AtomicU64::new(0),
            recent: Mutex::new(VecDeque::new()),
        }
    }

    pub fn transport(&self) -> Arc<dyn Transport> {
        self.transport.read().unwrap().clone()
    }

    pub fn set_transport(&self, t: Arc<dyn Transport>) {
        *self.transport.write().unwrap() = t;
    }

    fn note_call(&self) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let mut recent = self.recent.lock().unwrap();
        let now = Instant::now();
        recent.push_back(now);
        while recent
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60))
        {
            recent.pop_front();
        }
    }

    /// (calls since start, calls in the last minute): what the thermostat has had to answer.
    pub fn calls(&self) -> (u64, usize) {
        let mut recent = self.recent.lock().unwrap();
        let now = Instant::now();
        while recent
            .front()
            .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60))
        {
            recent.pop_front();
        }
        (self.calls.load(Ordering::Relaxed), recent.len())
    }

    /// Calls that can be repeated safely (reads, absolute writes).
    pub fn rpc(&self, query: &str, args: &[(&str, Arg)]) -> Result<Value> {
        self.note_call();
        self.transport().invoke(query, args, true)
    }

    /// Calls relative to the current state (setpoint steps, menu actions), never retried.
    pub fn rpc_once(&self, query: &str, args: &[(&str, Arg)]) -> Result<Value> {
        self.note_call();
        self.transport().invoke(query, args, false)
    }

    pub fn features(&self) -> Result<Map<String, Value>> {
        let r = self.rpc("synch_flags.json", &[])?;
        let flags = r["synch"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|f| f["flags"].as_u64())
            .unwrap_or(0);
        Ok(FEATURES
            .iter()
            .map(|(name, bit)| (name.to_string(), json!(flags >> bit & 1 == 1)))
            .collect())
    }

    pub fn room_list(&self) -> Result<Vec<Value>> {
        Ok(self.rpc("room_list.json", &[])?["rooms"]
            .as_array()
            .cloned()
            .unwrap_or_default())
    }

    /// Reads (all fields empty) or updates a room. Empty values mean "leave unchanged".
    pub fn room_state(&self, id: u16, change: &RoomChange) -> Result<RoomState> {
        let args = [
            ("id", int(id)),
            ("tdelta", int(change.tdelta.unwrap_or(0))),
            ("season", raw_opt(&change.season)),
            ("vacation", raw_opt(&change.vacation)),
            ("mode", raw_opt(&change.mode)),
            ("name", raw_opt(&change.name)),
        ];
        let r = if change.tdelta.unwrap_or(0) != 0 {
            self.rpc_once("room_state.json", &args)?
        } else {
            self.rpc("room_state.json", &args)?
        };
        Ok(serde_json::from_value(r)?)
    }

    pub fn versions(&self) -> Result<Value> {
        self.rpc("versions.json", &[])
    }

    pub fn message_log(&self) -> Result<Value> {
        Ok(self.rpc("message_log.json", &[])?["log"].clone())
    }

    /// Weekly room timer: 21 uint32 (3 per day, Monday first), 2 bits per half hour.
    /// Passing `day` + `bits` writes that day; day -1 only reads.
    pub fn room_timer(
        &self,
        id: u16,
        timer_type: u8,
        write: Option<(u8, [u32; 3])>,
    ) -> Result<Vec<u32>> {
        let (day, bf) = write.map(|(d, b)| (d as i64, b)).unwrap_or((-1, [0; 3]));
        let r = self.rpc(
            "room_timer.json",
            &[
                ("id", int(id as i64)),
                ("type", int(timer_type as i64)),
                ("day", int(day)),
                ("bf0", int(bf[0] as i64)),
                ("bf1", int(bf[1] as i64)),
                ("bf2", int(bf[2] as i64)),
            ],
        )?;
        timer_values(&r)
    }

    /// Copies one day of a room timer to the days in `mask` (bit 0 = Monday).
    pub fn room_timer_copy(&self, id: u16, timer_type: u8, day: u8, mask: u8) -> Result<Vec<u32>> {
        let r = self.rpc(
            "timer_copy.json",
            &[
                ("id", int(id as i64)),
                ("type", int(timer_type as i64)),
                ("day", int(day as i64)),
                ("mask", int(mask as i64)),
            ],
        )?;
        timer_values(&r)
    }

    // ---- device menu ("Experto" in the app). The device keeps a menu cursor; every
    // navigation is action_2.json with the current menu id and an action string.

    pub fn menu_action(&self, extended: bool, id: &str, action: &str) -> Result<Value> {
        let q = if extended {
            "action_2.json"
        } else {
            "action.json"
        };
        // the app always sends both, NUL-terminated even when empty
        let z = |s: &str| Arg::Bytes(format!("{s}\0").into_bytes());
        self.rpc_once(q, &[("id", z(id)), ("action", z(action))])
    }

    /// Reads the details of a menu screen by id. Does not move the device cursor.
    pub fn menu_detail(&self, extended: bool, menu_type: &str, id: &str) -> Result<Value> {
        let d = |q: &str, extra: &[(&str, Arg)]| -> Result<Value> {
            let mut params = vec![("id", raw(id))];
            params.extend_from_slice(extra);
            self.rpc(q, &params)
        };
        match menu_type {
            "string list" => d(
                if extended {
                    "string_list_2.json"
                } else {
                    "string_list.json"
                },
                &[],
            ),
            "dynamic list" | "dynamic list tiled" => d(
                if extended {
                    "dynamic_list_2.json"
                } else {
                    "dynamic_list.json"
                },
                &[],
            ),
            "options" => d("options.json", &[]),
            "offset" => d("offset.json", &[]),
            "information" => d("information.json", &[]),
            "binary query" => d("binary_query.json", &[]),
            "system date" => d(
                "system_date.json",
                &[("day", int(-1)), ("month", int(-1)), ("year", int(-1))],
            ),
            "month day" => d(
                "system_date_2.json",
                &[("day", int(-1)), ("month", int(-1)), ("year", int(-1))],
            ),
            "system time" => d(
                "system_time.json",
                &[("hour", int(-1)), ("minute", int(-1))],
            ),
            "system datetime" => d(
                "system_datetime.json",
                &[
                    ("day", int(-1)),
                    ("month", int(-1)),
                    ("year", int(-1)),
                    ("hour", int(-1)),
                    ("minute", int(-1)),
                ],
            ),
            "text" => d("text.json", &[("value", Arg::Bytes(b"\0".to_vec()))]),
            "timer" => d(
                "timer.json",
                &[
                    ("day", int(-1)),
                    ("bf0", int(0)),
                    ("bf1", int(0)),
                    ("bf2", int(0)),
                ],
            ),
            "time ranges" | "time ranges 2" => {
                let q = if menu_type == "time ranges" {
                    "time_ranges.json"
                } else {
                    "time_ranges_2.json"
                };
                // the app sends the id without the NUL terminator here
                self.rpc(
                    q,
                    &[
                        ("id", Arg::Bytes(id.as_bytes().to_vec())),
                        ("day", int(-1)),
                        ("timer", Arg::List(vec![])),
                    ],
                )
            }
            _ => Ok(json!({ "unsupported": menu_type })),
        }
    }

    /// Writes a value on a date/time/text/timer screen, same calls as the app.
    pub fn menu_set(&self, menu_type: &str, id: &str, v: &Value) -> Result<Value> {
        let get = |k: &str| -> Result<i64> { v[k].as_i64().ok_or_else(|| anyhow!("missing {k}")) };
        match menu_type {
            "system date" | "month day" => {
                let q = if menu_type == "system date" {
                    "system_date.json"
                } else {
                    "system_date_2.json"
                };
                self.rpc(
                    q,
                    &[
                        ("id", raw(id)),
                        ("day", int(get("day")?)),
                        ("month", int(get("month")?)),
                        ("year", int(get("year")?)),
                    ],
                )
            }
            "system time" => self.rpc(
                "system_time.json",
                &[
                    ("id", raw(id)),
                    ("hour", int(get("hour")?)),
                    ("minute", int(get("minute")?)),
                ],
            ),
            "system datetime" => self.rpc(
                "system_datetime.json",
                &[
                    ("id", raw(id)),
                    ("day", int(get("day")?)),
                    ("month", int(get("month")?)),
                    ("year", int(get("year")?)),
                    ("hour", int(get("hour")?)),
                    ("minute", int(get("minute")?)),
                ],
            ),
            "text" => {
                let s = v["value"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing value"))?;
                self.rpc("text.json", &[("id", raw(id)), ("value", raw(s))])
            }
            "timer" => {
                let day = get("day")?;
                let slots: Vec<String> = serde_json::from_value(v["slots"].clone())?;
                let bf = bitfield(&slots, &SERVICE_TIMER_MODES)?;
                let r = self.rpc(
                    "timer.json",
                    &[
                        ("id", raw(id)),
                        ("day", int(day)),
                        ("bf0", int(bf[0] as i64)),
                        ("bf1", int(bf[1] as i64)),
                        ("bf2", int(bf[2] as i64)),
                    ],
                )?;
                Ok(r)
            }
            "timer copy" => self.rpc(
                "timer_menu_copy.json",
                &[
                    ("id", raw(id)),
                    ("day", int(get("day")?)),
                    ("mask", int(get("mask")?)),
                ],
            ),
            other => bail!("cannot set values on a '{other}' screen"),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct RoomChange {
    pub tdelta: Option<i8>,
    pub season: Option<String>,
    pub vacation: Option<String>,
    pub mode: Option<String>,
    pub name: Option<String>,
}

fn timer_values(r: &Value) -> Result<Vec<u32>> {
    let list = r["timer"]
        .as_array()
        .ok_or_else(|| anyhow!("no timer in response"))?;
    Ok(list
        .iter()
        .map(|t| t["value"].as_u64().unwrap_or(0) as u32)
        .collect())
}

/// 21 uint32 -> 7 days x 48 slot names.
pub fn decode_week(values: &[u32], modes: &[&str]) -> Vec<Vec<String>> {
    (0..7)
        .map(|day| {
            (0..48)
                .map(|slot| {
                    let word = values.get(day * 3 + slot / 16).copied().unwrap_or(0);
                    let n = (word >> ((slot % 16) * 2) & 3) as usize;
                    modes.get(n).unwrap_or(&"?").to_string()
                })
                .collect()
        })
        .collect()
}

/// 48 slot names -> the 3 bitfields of one day.
pub fn bitfield(slots: &[String], modes: &[&str]) -> Result<[u32; 3]> {
    if slots.len() != 48 {
        bail!("a day has 48 half-hour slots, got {}", slots.len());
    }
    let mut bf = [0u32; 3];
    for (i, s) in slots.iter().enumerate() {
        let n = modes
            .iter()
            .position(|m| m == s)
            .ok_or_else(|| anyhow!("unknown slot mode {s}"))? as u32;
        bf[i / 16] |= n << ((i % 16) * 2);
    }
    Ok(bf)
}

pub fn timer_modes(room: u16) -> &'static [&'static str] {
    if room == SYSTEM_ROOM {
        &HWS_MODES
    } else {
        &ROOM_MODES
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn week_roundtrip() {
        let slots: Vec<String> = (0..48).map(|i| ROOM_MODES[i % 4].to_string()).collect();
        let bf = bitfield(&slots, &ROOM_MODES).unwrap();
        let mut values = vec![0u32; 21];
        values[3..6].copy_from_slice(&bf);
        assert_eq!(decode_week(&values, &ROOM_MODES)[1], slots);
    }
}

//! Optional transport through the closed-source Nabto 4.3 client SDK (feature "nabto-sdk").

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;
use std::ptr;

type Handle = *mut c_void;

extern "C" {
    fn nabtoStartup(home: *const c_char) -> c_int;
    fn nabtoInstallDefaultStaticResources(dir: *const c_char) -> c_int;
    fn nabtoVersionString(version: *mut *mut c_char) -> c_int;
    fn nabtoStatusStr(status: c_int) -> *const c_char;
    fn nabtoCreateSelfSignedProfile(id: *const c_char, password: *const c_char) -> c_int;
    fn nabtoGetFingerprint(id: *const c_char, fingerprint: *mut c_char) -> c_int;
    fn nabtoOpenSession(session: *mut Handle, id: *const c_char, password: *const c_char) -> c_int;
    fn nabtoCloseSession(session: Handle) -> c_int;
    fn nabtoRpcSetDefaultInterface(
        session: Handle,
        xml: *const c_char,
        err: *mut *mut c_char,
    ) -> c_int;
    fn nabtoRpcInvoke(session: Handle, url: *const c_char, json: *mut *mut c_char) -> c_int;
    fn nabtoFree(p: *mut c_void) -> c_int;
}

/// Status returned when the device answered with a JSON error body.
pub const STATUS_FAILED_WITH_JSON_MESSAGE: i32 = 26;

#[derive(Debug)]
pub struct NabtoError {
    pub status: i32,
    pub message: String,
}

impl std::fmt::Display for NabtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (nabto status {})", self.message, self.status)
    }
}

impl std::error::Error for NabtoError {}

fn status_str(status: c_int) -> String {
    unsafe {
        CStr::from_ptr(nabtoStatusStr(status))
            .to_string_lossy()
            .into_owned()
    }
}

fn check(status: c_int, what: &str) -> Result<(), NabtoError> {
    if status == 0 {
        Ok(())
    } else {
        Err(NabtoError {
            status,
            message: format!("{what}: {}", status_str(status)),
        })
    }
}

fn cstring(s: &str) -> CString {
    CString::new(s).expect("string with interior NUL")
}

/// Takes ownership of a string allocated by the SDK.
unsafe fn take_string(p: *mut c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    let s = CStr::from_ptr(p).to_string_lossy().into_owned();
    nabtoFree(p as *mut c_void);
    Some(s)
}

/// Process-wide SDK initialisation. nabtoShutdown is never called because it can block for a
/// long time, so the process exits instead.
pub fn startup(home: &Path) -> Result<String, NabtoError> {
    std::fs::create_dir_all(home).ok();
    let home = cstring(&home.to_string_lossy());
    unsafe {
        check(nabtoStartup(home.as_ptr()), "startup")?;
        nabtoInstallDefaultStaticResources(ptr::null());
        let mut v = ptr::null_mut();
        nabtoVersionString(&mut v);
        Ok(take_string(v).unwrap_or_default())
    }
}

/// Creates the self-signed profile if it does not exist yet and returns its fingerprint.
pub fn ensure_profile(home: &Path, user: &str, password: &str) -> Result<String, NabtoError> {
    let user_c = cstring(user);
    if !home.join("users").join(format!("{user}.crt")).exists() {
        tracing::info!("creating self-signed Nabto profile {user}");
        let pw = cstring(password);
        unsafe {
            check(
                nabtoCreateSelfSignedProfile(user_c.as_ptr(), pw.as_ptr()),
                "create profile",
            )?
        };
    }
    let mut fp = [0 as c_char; 16];
    unsafe {
        check(
            nabtoGetFingerprint(user_c.as_ptr(), fp.as_mut_ptr()),
            "fingerprint",
        )?
    };
    Ok(fp
        .iter()
        .map(|b| format!("{:02x}", *b as u8))
        .collect::<Vec<_>>()
        .join(":"))
}

pub struct Session {
    handle: Handle,
}

// The SDK handle is used from one thread at a time (callers hold a mutex).
unsafe impl Send for Session {}

impl Session {
    pub fn open(user: &str, password: &str, interface_xml: &str) -> Result<Session, NabtoError> {
        let mut handle: Handle = ptr::null_mut();
        let (u, p, xml) = (cstring(user), cstring(password), cstring(interface_xml));
        unsafe {
            check(
                nabtoOpenSession(&mut handle, u.as_ptr(), p.as_ptr()),
                "open session",
            )?;
            let mut err = ptr::null_mut();
            let status = nabtoRpcSetDefaultInterface(handle, xml.as_ptr(), &mut err);
            let msg = take_string(err);
            if status != 0 {
                nabtoCloseSession(handle);
                return Err(NabtoError {
                    status,
                    message: format!("set interface: {}", msg.unwrap_or_default()),
                });
            }
        }
        Ok(Session { handle })
    }

    /// Invokes `nabto://...` and returns the raw JSON body (on success or on a JSON error).
    pub fn invoke(&self, url: &str) -> (i32, Option<String>) {
        let url = cstring(url);
        let mut out = ptr::null_mut();
        unsafe {
            let status = nabtoRpcInvoke(self.handle, url.as_ptr(), &mut out);
            (status, take_string(out))
        }
    }

    pub fn status_message(status: i32) -> String {
        status_str(status)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            nabtoCloseSession(self.handle);
        }
    }
}

// ---- Transport over the official SDK (feature "nabto-sdk")

use crate::schema::Arg;
use crate::transport::Transport;
use serde_json::{Map, Value};
use std::sync::{Mutex, OnceLock};

pub const PROFILE: &str = "caleon-nas";
const PROFILE_PASSWORD: &str = "caleon-nas";
static STARTED: OnceLock<Result<String, String>> = OnceLock::new();

/// Starts the SDK once and makes sure the self-signed profile exists.
pub fn init(home: &Path) -> anyhow::Result<String> {
    STARTED
        .get_or_init(|| {
            let v = startup(home).map_err(|e| e.to_string())?;
            let fp = ensure_profile(home, PROFILE, PROFILE_PASSWORD).map_err(|e| e.to_string())?;
            tracing::info!("Nabto SDK {v}, profile {PROFILE} fingerprint {fp}");
            Ok(v)
        })
        .clone()
        .map_err(anyhow::Error::msg)
}

pub struct SdkTransport {
    device: String,
    interface_xml: String,
    session: Mutex<Option<Session>>,
}

impl SdkTransport {
    pub fn new(device: String, interface_xml: String) -> SdkTransport {
        SdkTransport {
            device,
            interface_xml,
            session: Mutex::new(None),
        }
    }
}

/// encodeURIComponent, which is what the app applies to every value.
fn encode(bytes: &[u8]) -> String {
    let mut out = String::new();
    for &b in bytes {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl Transport for SdkTransport {
    fn kind(&self) -> &'static str {
        "sdk"
    }

    fn invoke(&self, query: &str, args: &[(&str, Arg)], retry: bool) -> anyhow::Result<Value> {
        let url = if args.iter().any(|(_, a)| matches!(a, Arg::List(_))) {
            // the app sends requests with lists as one `json` parameter
            let mut req = Map::new();
            for (k, a) in args {
                req.insert(
                    k.to_string(),
                    match a {
                        Arg::Int(n) => Value::from(*n),
                        Arg::Bytes(b) => Value::from(
                            String::from_utf8_lossy(b.strip_suffix(&[0]).unwrap_or(b)).into_owned(),
                        ),
                        Arg::List(l) => {
                            Value::from(l.iter().cloned().map(Value::Object).collect::<Vec<_>>())
                        }
                    },
                );
            }
            let json = serde_json::json!({ "request": req }).to_string();
            format!(
                "nabto://{}/{}?json={}",
                self.device,
                query,
                encode(json.as_bytes())
            )
        } else {
            let qs: Vec<String> = args
                .iter()
                .map(|(k, a)| match a {
                    Arg::Int(n) => format!("{k}={n}"),
                    Arg::Bytes(b) => format!("{k}={}", encode(b)),
                    Arg::List(_) => unreachable!(),
                })
                .collect();
            format!("nabto://{}/{}?{}", self.device, query, qs.join("&"))
        };
        let mut guard = self.session.lock().unwrap();
        let attempts = if retry { 3 } else { 1 };
        let mut last = None;
        for _ in 0..attempts {
            if guard.is_none() {
                match Session::open(PROFILE, PROFILE_PASSWORD, &self.interface_xml) {
                    Ok(s) => *guard = Some(s),
                    Err(e) => {
                        last = Some(e);
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        continue;
                    }
                }
            }
            let (status, body) = guard.as_ref().unwrap().invoke(&url);
            let v: Value = body
                .as_deref()
                .and_then(|b| serde_json::from_str(b).ok())
                .unwrap_or(Value::Null);
            if status == 0 {
                return Ok(v["response"].clone());
            }
            let detail = v["error"]["detail"]
                .as_str()
                .filter(|s| !s.is_empty())
                .or(v["error"]["header"].as_str());
            let err = NabtoError {
                status,
                message: detail
                    .map(String::from)
                    .unwrap_or_else(|| Session::status_message(status)),
            };
            let transient = detail.map_or(true, |d| {
                d.contains("exchanging data") || d.contains("(2000058)") || d == self.device
            });
            if status == STATUS_FAILED_WITH_JSON_MESSAGE && !transient {
                return Err(err.into());
            }
            if status != STATUS_FAILED_WITH_JSON_MESSAGE {
                *guard = None;
            }
            last = Some(err);
            std::thread::sleep(std::time::Duration::from_millis(700));
        }
        Err(last
            .map(Into::into)
            .unwrap_or_else(|| anyhow::anyhow!("{query} failed")))
    }
}

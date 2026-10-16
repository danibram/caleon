//! Connection settings (persisted in DATA_DIR/settings.json) and transport construction.

use anyhow::{bail, Result};
use caleon::schema::Schema;
use caleon::transport::{Found, SessionTransport, Transport, UdpTransport};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Settings {
    /// "udp" (local protocol, pure Rust) or "sdk" (official Nabto client, optional feature).
    pub transport: String,
    /// Fixed IP of the thermostat; empty means broadcast discovery.
    #[serde(default)]
    pub host: String,
    /// Authorized e-mail for the "session" transport (must be in the device access list).
    #[serde(default)]
    pub email: String,
}

impl Settings {
    fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("settings.json")
    }

    /// settings.json if it exists, else the TRANSPORT / CALEON_HOST environment variables.
    pub fn load(data_dir: &Path) -> Settings {
        std::fs::read(Self::path(data_dir))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_else(|| Settings {
                transport: std::env::var("TRANSPORT").unwrap_or_else(|_| "udp".into()),
                host: std::env::var("CALEON_HOST").unwrap_or_default(),
                email: std::env::var("CALEON_EMAIL").unwrap_or_default(),
            })
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        std::fs::write(Self::path(data_dir), serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }
}

/// Transports this build can use.
pub fn available() -> Vec<&'static str> {
    let mut v = vec!["udp", "session"];
    if cfg!(feature = "nabto-sdk") {
        v.push("sdk");
    }
    v
}

pub fn build(
    settings: &Settings,
    found: &Found,
    schema: &Arc<Schema>,
    data_dir: &Path,
) -> Result<Arc<dyn Transport>> {
    let addr = SocketAddr::new(found.addr.ip(), found.addr.port());
    match settings.transport.as_str() {
        "udp" => Ok(Arc::new(UdpTransport::new(addr, schema.clone())?)),
        "session" => {
            if settings.email.trim().is_empty() {
                bail!("el modo con sesión necesita un correo autorizado del equipo (ponlo en Ajustes)");
            }
            Ok(Arc::new(SessionTransport::new(
                addr,
                settings.email.trim().to_string(),
                schema.clone(),
            )))
        }
        #[cfg(feature = "nabto-sdk")]
        "sdk" => {
            caleon::nabto::init(&data_dir.join("nabto"))?;
            Ok(Arc::new(caleon::nabto::SdkTransport::new(
                found.id.clone(),
                caleon::INTERFACE_XML.to_string(),
            )))
        }
        #[cfg(not(feature = "nabto-sdk"))]
        "sdk" => {
            let _ = data_dir;
            Err(anyhow::anyhow!(
                "este binario se compiló sin el SDK de Nabto (cargo build --features nabto-sdk)"
            ))
        }
        other => bail!("modo de conexión desconocido: {other}"),
    }
}

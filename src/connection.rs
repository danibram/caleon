//! Connection settings (persisted in DATA_DIR/settings.json) and transport construction.

use crate::schema::Schema;
use crate::transport::{self, Found, SessionTransport, Transport, UdpTransport};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use ts_rs::TS;

pub const INTERFACE_XML: &str = include_str!("../unabto_queries.xml");

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

/// Finds the thermostat: asks the fixed host if there is one, otherwise broadcasts. When
/// `want_id` is set, only that device is accepted.
pub fn locate(host: &str, want_id: &str) -> Result<Found> {
    let target = if host.is_empty() {
        None
    } else {
        Some(
            host.parse::<Ipv4Addr>()
                .with_context(|| format!("IP no válida: {host}"))?,
        )
    };
    for attempt in 1..=5 {
        let found = transport::discover(target, Duration::from_secs(2))?;
        tracing::info!("discovery: {found:?}");
        let pick = found.into_iter().find(|f| {
            (want_id.is_empty() || f.id == want_id) && f.id.ends_with(".sorel-connect.net")
        });
        if let Some(f) = pick {
            return Ok(f);
        }
        if attempt < 5 {
            std::thread::sleep(Duration::from_secs(1));
        }
    }
    match target {
        Some(ip) => bail!(
            "el equipo en {ip} no responde en el puerto {}",
            transport::PORT
        ),
        None => bail!(
            "no se encontró ningún °CALEON en la red; indica su IP en Ajustes o con CALEON_HOST"
        ),
    }
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
            crate::nabto::init(&data_dir.join("nabto"))?;
            Ok(Arc::new(crate::nabto::SdkTransport::new(
                found.id.clone(),
                INTERFACE_XML.to_string(),
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

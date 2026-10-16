//! Client for SOREL °CALEON thermostats over the local uNabto protocol: discovery, the UDP and
//! session transports (plus the official SDK behind the `nabto-sdk` feature), the room, timer
//! and expert-menu calls of the official app, and the parser of the device status screens.
//!
//! ```no_run
//! use std::sync::Arc;
//! use caleon::device::{Device, RoomChange};
//! use caleon::transport::UdpTransport;
//!
//! # fn main() -> anyhow::Result<()> {
//! let found = caleon::locate("", "")?; // broadcast discovery, or an IP and a device id
//! let transport = UdpTransport::new(found.addr, Arc::new(caleon::schema()?))?;
//! let device = Device::new(found.id, Arc::new(transport));
//! for (id, room) in device.room_list()?.iter().enumerate() {
//!     let state = device.room_state(id as u16, &RoomChange::default())?;
//!     println!("{}: {} ({})", room["name"], room["temperature"], state.mode);
//! }
//! # Ok(())
//! # }
//! ```

pub mod device;
#[cfg(feature = "nabto-sdk")]
pub mod nabto;
pub mod schema;
pub mod status;
pub mod transport;

pub use device::{Device, RoomChange, RoomState};
pub use schema::Schema;
pub use transport::{Found, Transport};

use anyhow::{bail, Context, Result};
use std::net::Ipv4Addr;
use std::time::Duration;

/// The device's RPC interface definition (`unabto_queries.xml`), as carried by the official app.
pub const INTERFACE_XML: &str = include_str!("../unabto_queries.xml");

/// The parsed interface definition.
pub fn schema() -> Result<Schema> {
    Schema::parse(INTERFACE_XML)
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

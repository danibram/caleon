//! How RPC calls reach the device.
//!
//! `UdpTransport` speaks the uNabto local legacy protocol (enabled by default in uNabto as
//! NABTO_ENABLE_LOCAL_ACCESS_LEGACY_PROTOCOL): one UDP datagram per request to port 5570,
//! no session and no encryption. It is pure Rust and works on any architecture.
//!
//! Packet: <hdr u32> <seq u32> <nabto u32> <query id u32> <parameters>. The device answers with
//! the same header plus the RSP flag (and ERR when the application rejected the request),
//! followed by the encoded response.

use crate::schema::{self, Arg, Schema};
use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const PORT: u16 = 5570;
const TYPE_APPLICATION: u32 = 0x00;
const TYPE_DISCOVERY: u32 = 0x01;
const FLAG_RSP: u32 = 0x0080_0000;
const FLAG_ERR: u32 = 0x0040_0000;
const TIMEOUT: Duration = Duration::from_millis(2500);

pub trait Transport: Send + Sync {
    /// Short identifier shown in the settings ("udp", "sdk").
    fn kind(&self) -> &'static str;
    /// Calls `query` and returns its decoded response. `retry` must be false for requests that
    /// change something relative to the current state (a lost answer would apply them twice).
    fn invoke(&self, query: &str, args: &[(&str, Arg)], retry: bool) -> Result<Value>;
}

pub struct UdpTransport {
    pub addr: SocketAddr,
    schema: Arc<Schema>,
    state: Mutex<(UdpSocket, u32)>,
}

impl UdpTransport {
    pub fn new(addr: SocketAddr, schema: Arc<Schema>) -> Result<UdpTransport> {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        let seq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .subsec_nanos();
        Ok(UdpTransport {
            addr,
            schema,
            state: Mutex::new((sock, seq)),
        })
    }
}

impl Transport for UdpTransport {
    fn kind(&self) -> &'static str {
        "udp"
    }

    fn invoke(&self, query: &str, args: &[(&str, Arg)], retry: bool) -> Result<Value> {
        let q = self.schema.get(query)?;
        let body = schema::encode_request(q, args)?;
        let mut guard = self.state.lock().unwrap();
        let (sock, seq) = &mut *guard;
        let attempts = if retry { 3 } else { 1 };
        for attempt in 1..=attempts {
            *seq = seq.wrapping_add(1);
            let mut pkt = Vec::with_capacity(12 + body.len());
            pkt.extend_from_slice(&TYPE_APPLICATION.to_be_bytes());
            pkt.extend_from_slice(&seq.to_be_bytes());
            pkt.extend_from_slice(&0u32.to_be_bytes());
            pkt.extend_from_slice(&body);
            sock.send_to(&pkt, self.addr)?;
            match recv_matching(sock, self.addr, *seq) {
                Ok((hdr, payload)) => {
                    if hdr & FLAG_ERR != 0 {
                        bail!("el equipo rechazó {query}");
                    }
                    return schema::decode_response(q, &payload);
                }
                Err(e) if attempt < attempts => tracing::debug!("{query}: {e}, retrying"),
                Err(e) => bail!("{query}: {e}"),
            }
        }
        unreachable!()
    }
}

/// Waits for the response to `seq`, ignoring late answers to earlier requests.
fn recv_matching(sock: &UdpSocket, from: SocketAddr, seq: u32) -> Result<(u32, Vec<u8>)> {
    let deadline = Instant::now() + TIMEOUT;
    let mut buf = vec![0u8; 65536];
    loop {
        let left = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| anyhow!("sin respuesta del equipo"))?;
        sock.set_read_timeout(Some(left))?;
        let (n, peer) = match sock.recv_from(&mut buf) {
            Ok(r) => r,
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                bail!("sin respuesta del equipo")
            }
            Err(e) => return Err(e.into()),
        };
        if peer.ip() != from.ip() || n < 12 {
            continue;
        }
        let hdr = u32::from_be_bytes(buf[0..4].try_into()?);
        let got = u32::from_be_bytes(buf[4..8].try_into()?);
        if got == seq && hdr & FLAG_RSP != 0 && hdr & 0xff == TYPE_APPLICATION {
            return Ok((hdr, buf[12..n].to_vec()));
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Found {
    pub id: String,
    pub addr: SocketAddr,
}

/// Local discovery that asks every device (or the one at `target`) to announce itself.
pub fn discover(target: Option<Ipv4Addr>, wait: Duration) -> Result<Vec<Found>> {
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.set_broadcast(true)?;
    let mut pkt = Vec::new();
    pkt.extend_from_slice(&TYPE_DISCOVERY.to_be_bytes());
    pkt.extend_from_slice(&1u32.to_be_bytes());
    pkt.extend_from_slice(&0u32.to_be_bytes());
    pkt.extend_from_slice(b"*\0");
    let to = SocketAddrV4::new(target.unwrap_or(Ipv4Addr::BROADCAST), PORT);
    sock.send_to(&pkt, to)?;
    let deadline = Instant::now() + wait;
    let mut out: Vec<Found> = Vec::new();
    let mut buf = [0u8; 2048];
    while let Some(left) = deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
    {
        sock.set_read_timeout(Some(left))?;
        let Ok((n, peer)) = sock.recv_from(&mut buf) else {
            break;
        };
        // <hdr><seq><nabto> <ip u32> <port u16> <type u8> <device id> NUL ...
        if n < 20 || u32::from_be_bytes(buf[0..4].try_into()?) & 0xff != TYPE_DISCOVERY {
            continue;
        }
        let port = u16::from_be_bytes([buf[16], buf[17]]);
        let id_bytes = &buf[19..n];
        let id = String::from_utf8_lossy(
            &id_bytes[..id_bytes
                .iter()
                .position(|b| *b == 0)
                .unwrap_or(id_bytes.len())],
        )
        .into_owned();
        let addr = SocketAddr::new(peer.ip(), if port == 0 { PORT } else { port });
        if !id.is_empty() && !out.iter().any(|f| f.id == id) {
            out.push(Found { id, addr });
        }
    }
    Ok(out)
}

// ---- Local connection with a session (uNabto U_CONNECT + null-crypto DATA).
//
// Unlike the legacy protocol this one has access control. The device only accepts clients whose
// e-mail is in its "Control de acceso" list. The handshake is U_CONNECT with an IPX and a CP_ID
// (e-mail) payload; the device replies with NOTIFY connect-ok and a session id. Each later RPC is
// a DATA packet carrying a CRYPT payload with crypto suite 0x000A (no encryption), a 2-byte
// additive checksum trailer, and 1-2 bytes of padding. Wire details verified against the device
// and cross-checked with the open proxon/genvex clients.
const TYPE_U_CONNECT: u8 = 0x83;
const TYPE_DATA: u8 = 0x16;
const FLAG_EXCEPTION: u8 = 0x02;
const PL_IPX: u8 = 0x35;
const PL_CRYPT: u8 = 0x36;
const PL_NOTIFY: u8 = 0x34;
const PL_CP_ID: u8 = 0x3F;
const CRYPT_NULL: u16 = 0x000A;
const NOTIFY_CONNECT_OK: u32 = 0x0001;
const MAX_RPCS_PER_SESSION: u32 = 150;
const MIN_SEND_INTERVAL: Duration = Duration::from_millis(250);

fn payload(ptype: u8, data: &[u8]) -> Vec<u8> {
    let mut v = vec![ptype, 0];
    v.extend_from_slice(&((4 + data.len()) as u16).to_be_bytes());
    v.extend_from_slice(data);
    v
}

/// Wraps request bytes in a null-crypto CRYPT payload: crypto code, 1-2 bytes of padding and a
/// checksum trailer. The payload length field counts that trailer, hence the `-2` when decoding.
fn crypt_null(data: &[u8]) -> Vec<u8> {
    let pad = 2 - (data.len() % 2); // 1 when odd, 2 when even: always present, always 1..=2
    let mut body = data.to_vec();
    body.extend(std::iter::repeat_n(pad as u8, pad));
    let len = (4 + 2 + body.len() + 2) as u16;
    let mut v = vec![PL_CRYPT, 0];
    v.extend_from_slice(&len.to_be_bytes());
    v.extend_from_slice(&CRYPT_NULL.to_be_bytes());
    v.extend_from_slice(&body);
    v
}

struct Session {
    sock: UdpSocket,
    client_id: [u8; 4],
    server_id: [u8; 4],
    seq: u16,
    rpcs: u32,
    last_send: Instant,
}

pub struct SessionTransport {
    pub addr: SocketAddr,
    email: String,
    schema: Arc<Schema>,
    session: Mutex<Option<Session>>,
}

impl SessionTransport {
    pub fn new(addr: SocketAddr, email: String, schema: Arc<Schema>) -> SessionTransport {
        SessionTransport {
            addr,
            email,
            schema,
            session: Mutex::new(None),
        }
    }
}

impl Session {
    fn pace(&mut self) {
        if let Some(w) = MIN_SEND_INTERVAL.checked_sub(self.last_send.elapsed()) {
            std::thread::sleep(w);
        }
        self.last_send = Instant::now();
    }

    fn header(&self, ptype: u8, seq: u16, payloads_len: usize, checksum: bool) -> Vec<u8> {
        let mut h = Vec::with_capacity(16);
        h.extend_from_slice(&self.client_id);
        h.extend_from_slice(&self.server_id);
        h.extend_from_slice(&[ptype, 0x02, 0x00, 0x00]);
        h.extend_from_slice(&seq.to_be_bytes());
        h.extend_from_slice(
            &((16 + payloads_len + if checksum { 2 } else { 0 }) as u16).to_be_bytes(),
        );
        h
    }

    /// Reads the response to `seq` of `ptype`, ignoring late answers to earlier requests.
    fn recv(&self, ptype: u8, seq: u16) -> Result<(u8, Vec<u8>)> {
        let deadline = Instant::now() + TIMEOUT;
        let mut buf = vec![0u8; 65536];
        loop {
            let left = deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or_else(|| anyhow!("sin respuesta del equipo"))?;
            self.sock.set_read_timeout(Some(left))?;
            let (n, _) = match self.sock.recv_from(&mut buf) {
                Ok(r) => r,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    bail!("sin respuesta del equipo")
                }
                Err(e) => return Err(e.into()),
            };
            if n >= 16
                && buf[0..4] == self.client_id
                && buf[8] == ptype
                && u16::from_be_bytes([buf[12], buf[13]]) == seq
            {
                return Ok((buf[11], buf[..n].to_vec()));
            }
        }
    }
}

impl SessionTransport {
    fn connect(&self) -> Result<Session> {
        let sock = UdpSocket::bind("0.0.0.0:0")?;
        let client_id: [u8; 4] = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .subsec_nanos()
            .to_be_bytes();
        let mut s = Session {
            sock,
            client_id,
            server_id: [0; 4],
            seq: 1,
            rpcs: 0,
            last_send: Instant::now(),
        };
        let mut payloads = payload(PL_IPX, &[&[0u8; 12][..], &[0x80]].concat());
        payloads.extend(payload(
            PL_CP_ID,
            &[b"\x01", self.email.as_bytes()].concat(),
        ));
        let mut pkt = s.header(TYPE_U_CONNECT, 1, payloads.len(), false);
        pkt.extend_from_slice(&payloads);
        for attempt in 1..=5 {
            s.pace();
            s.sock.send_to(&pkt, self.addr)?;
            match s.recv(TYPE_U_CONNECT, 1) {
                Ok((_, resp)) => {
                    // NOTIFY payload at offset 16: [type][0][len u16][notify u32][session id u32]
                    if resp.len() < 28 || resp[16] != PL_NOTIFY {
                        bail!("respuesta de conexión inesperada");
                    }
                    let notify = u32::from_be_bytes(resp[20..24].try_into()?);
                    if notify != NOTIFY_CONNECT_OK {
                        bail!(connect_error(notify, &self.email));
                    }
                    s.server_id = resp[24..28].try_into()?;
                    return Ok(s);
                }
                Err(e) if attempt < 5 => tracing::debug!("connect attempt {attempt}: {e}"),
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    fn call(&self, s: &mut Session, body: &[u8]) -> Result<Vec<u8>> {
        s.seq = s.seq.wrapping_add(1);
        s.rpcs += 1;
        let crypt = crypt_null(body);
        let mut pkt = s.header(TYPE_DATA, s.seq, crypt.len(), true);
        pkt.extend_from_slice(&crypt);
        let sum = pkt.iter().fold(0u32, |a, b| a + *b as u32) as u16;
        pkt.extend_from_slice(&sum.to_be_bytes());
        s.pace();
        s.sock.send_to(&pkt, self.addr)?;
        let (flags, resp) = s.recv(TYPE_DATA, s.seq)?;
        if resp.len() < 22 || resp[16] != PL_CRYPT {
            bail!("respuesta sin carga CRYPT");
        }
        let plen = u16::from_be_bytes([resp[18], resp[19]]) as usize;
        let end = 16 + plen.checked_sub(2).ok_or_else(|| anyhow!("CRYPT corto"))?;
        let raw = resp
            .get(22..end)
            .ok_or_else(|| anyhow!("CRYPT fuera de rango"))?;
        let pad = *raw.last().ok_or_else(|| anyhow!("CRYPT vacío"))? as usize;
        let data = raw
            .get(..raw.len().saturating_sub(pad.clamp(1, 2)))
            .unwrap_or(&[]);
        if flags & FLAG_EXCEPTION != 0 {
            let aer = data
                .get(..4)
                .map(|b| u32::from_be_bytes(b.try_into().unwrap()))
                .unwrap_or(0);
            bail!("el equipo rechazó la consulta (código {aer})");
        }
        Ok(data.to_vec())
    }
}

fn connect_error(notify: u32, email: &str) -> String {
    match notify {
        0x8004 | 0x800D => format!("el equipo no autoriza a «{email}»; añade ese correo en Experto › Ajustes › WiFi › Control de acceso"),
        0x8009 => "el equipo está ocupado, inténtalo de nuevo".into(),
        other => format!("conexión rechazada (NOTIFY {other:#06x})"),
    }
}

impl Transport for SessionTransport {
    fn kind(&self) -> &'static str {
        "session"
    }

    fn invoke(&self, query: &str, args: &[(&str, Arg)], retry: bool) -> Result<Value> {
        let q = self.schema.get(query)?;
        let body = schema::encode_request(q, args)?;
        let mut guard = self.session.lock().unwrap();
        let attempts = if retry { 3 } else { 1 };
        let mut last = None;
        for attempt in 1..=attempts {
            if guard
                .as_ref()
                .is_none_or(|s| s.rpcs >= MAX_RPCS_PER_SESSION)
            {
                match self.connect() {
                    Ok(s) => *guard = Some(s),
                    Err(e) => {
                        // access denied is permanent; do not keep retrying it
                        if e.to_string().contains("no autoriza") {
                            return Err(e);
                        }
                        last = Some(e);
                        std::thread::sleep(Duration::from_millis(500));
                        continue;
                    }
                }
            }
            match self.call(guard.as_mut().unwrap(), &body) {
                Ok(data) => return schema::decode_response(q, &data),
                Err(e) if e.to_string().contains("rechazó") => return Err(e), // device-level error, not transport
                Err(e) => {
                    tracing::debug!("{query} attempt {attempt}: {e}, reconnecting");
                    *guard = None;
                    last = Some(e);
                }
            }
        }
        Err(last.unwrap_or_else(|| anyhow!("{query} failed")))
    }
}

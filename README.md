# caleon

[Español](README.es.md)

A Rust server to monitor and control a SOREL °CALEON thermostat from your own network, with no Sorel app and
no Sorel cloud. Made to run on a NAS with Docker. It is pure Rust and does not need the Nabto SDK.

> Independent project, not affiliated with SOREL or Nabto. The protocol comes from watching how the official
> app talks to the device. It writes to your heating, so use it at your own risk.

The web UI covers everything the official app does (°CALEON 7.0.36) and a bit more. It is designed for the
phone first (bottom tab bar, light and dark theme, page transitions) and works the same on a desktop. The UI
text is in Spanish.

<p align="center">
  <img src="docs/screenshots/home.png" width="180" alt="Home: rooms, modes and today's temperatures">
  <img src="docs/screenshots/room.png" width="180" alt="A room: gauge, setpoint and program">
  <img src="docs/screenshots/schedule.png" width="180" alt="Weekly schedule editor">
  <img src="docs/screenshots/charts.png" width="180" alt="Charts">
</p>
<p align="center"><img src="docs/screenshots/desktop.png" width="720" alt="Home on a desktop"></p>

- Home: rooms with temperature, humidity, setpoint (+/−), mode (Off, Normal, Eco, Comfort, with the Comfort
  countdown) and the global control for all rooms. Shows "Heating" when the relay of that room's zone is on.
- Room: schedules, holidays (start and end), season change when the device allows it, name, notices, expert
  menu, status and its history. The setpoint of each mode (Normal, Eco, Comfort) is edited in one place: the
  thermostat only keeps one setpoint per mode and only moves the active one, so the server visits each mode
  for a moment and comes back to the mode the room had.
- Schedules: all 7 days in half-hour slots for heating and cooling, painted with the mouse or a finger, plus
  "copy to next day / Monday-Friday / Saturday-Sunday / Monday-Sunday".
- Expert: the device's whole menu (language, date and time, devices, rooms, °Cbox functions, zones, WiFi,
  service values, lock, factory values...). It is the same remote menu the app draws. The °CALEON sends each
  screen and the web renders it.
- Status: inputs (1-Wire sensors, humidity, PWM), outputs (pump and zone relays, with the room each zone heats)
  and the circuit pump. Inputs and outputs are named after the CAN device they belong to (°CALEONbox or the
  thermostat itself).
- Charts: temperature and setpoint per room, humidity, every device sensor and a timeline of when each relay
  was on. 6 h, 24 h, 7 d and 30 d. The thermostat's own sensors read to 0.1 but are rated ±1 °C and ±6 % RH.
- Backups: the whole configuration as JSON (manual, or daily only when something changed), download, upload,
  comparison with the device and restore of names and schedules.
- Notices: the device's message log, firmware and enabled features.
- Settings: connection mode (direct, session or SDK) and the thermostat's IP.

## How it talks to the thermostat

The °CALEON uses Nabto 4 (uNabto). Besides the encrypted protocol the app uses through the cloud, uNabto ships
with a local protocol enabled by default (`NABTO_ENABLE_LOCAL_ACCESS_LEGACY_PROTOCOL`): each RPC query is one
UDP packet to port 5570, with no session and no encryption. The server speaks that protocol in pure Rust
(`src/transport.rs`) and encodes each query according to `unabto_queries.xml` (`src/schema.rs`). The answers
come out with the same JSON shape the official SDK returns.

It depends on no closed library, so it runs on any architecture: x86_64 or ARM NAS, Raspberry Pi and Mac,
all native.

The connection mode is chosen in Settings:

- **Direct over the local network** (default): the UDP protocol above, with no access control.
- **Session with e-mail**: uNabto's local connection with a session (`U_CONNECT` plus `DATA` packets with the
  null cipher). Same as direct but with access control, so it needs an e-mail that is authorized on the device
  (Expert › Settings › WiFi › Access control, up to five). It is a fallback in case the direct mode ever stops working.
- **Official Nabto SDK**: the closed Nabto 4.3 library, x86_64 only. Only offered when the binary was built
  with `--features nabto-sdk` (see below).

The change is applied only if the thermostat answers in the new mode, and it is saved in
`DATA_DIR/settings.json`.

Any computer on your network can talk to the thermostat this way, with no password. That is how it ships from
the factory; this server does not open anything. Keep it in mind if your network has guests.

### Nabto SDK (optional)

A fallback mode that talks to the device through the official Nabto 4.3 client library instead of the UDP
protocol written in Rust. In testing, a full backup taken through the SDK and one taken in direct mode came out
identical, so it is only worth using if the direct mode ever stops working, or to compare results.

**Getting the library.** It is closed source and is not in this repository. Nabto ships it inside its Python
wrapper, [nabto/nabto4-python-wrapper](https://github.com/nabto/nabto4-python-wrapper): the folders
`extension/linux64/lib` and `extension/mac64/lib` hold `libnabto_client_api_static.a` and
`libnabto_static_external.a`. Copy them with this layout, or point `NABTO_SDK_DIR` at a directory that has
it:

```
vendor/nabto4/linux64/lib/libnabto_client_api_static.a
vendor/nabto4/linux64/lib/libnabto_static_external.a
vendor/nabto4/mac64/lib/libnabto_client_api_static.a
vendor/nabto4/mac64/lib/libnabto_static_external.a
```

`vendor/` is in `.gitignore`. The header is not needed, because the functions in use are declared in `src/nabto.rs`.
The terms of use of those libraries are Nabto's, not this project's.

**Building.**

```sh
# Linux x86_64 (needs GNU ld, the binutils package)
cargo build --release -p caleon-server --features nabto-sdk

# macOS: the library is Intel only; on Apple Silicon it runs under Rosetta
rustup target add x86_64-apple-darwin
cargo build --release -p caleon-server --target x86_64-apple-darwin --features nabto-sdk

# Docker (amd64 only), with vendor/nabto4 already copied next to the Dockerfile
docker build --platform linux/amd64 --build-arg FEATURES=nabto-sdk -t caleon:latest .
```

If the library is missing, `build.rs` stops the build and names the path it looked in.

**Using it.** With a binary built this way, Settings offers the "Official Nabto SDK" mode; `TRANSPORT=sdk`
also selects it at startup. The first time, the SDK creates a self-signed profile (`caleon-nas`) in
`DATA_DIR/nabto/` and reuses it afterwards. Reads are retried up to three times; relative changes (setpoint,
menu actions) are sent once, as in the other modes.

**Limitations.**

- x86_64 only. Nabto publishes the library for 64-bit Intel Linux and macOS, so there is no SDK mode on an ARM
  NAS, a Raspberry Pi or Apple Silicon natively. The direct and session modes have no such limit.
- On Linux it must be linked with GNU ld (`build.rs` adds `-fuse-ld=bfd`), because the SDK was built with GCC 4.9 and
  registers constructors in `.ctors` sections, which `lld`, rustc's default linker, does not run.
- It is closed source, so it cannot be audited or fixed.
- CI does not build it, since it has no copy of the library, so a change can break it unnoticed until someone
  builds it by hand.
- On shutdown the process exits without calling `nabtoShutdown`, which can block for a long time. The profile
  is already on disk, so nothing is lost.
- The SDK only replaces the queries. The server still finds the device by UDP discovery (or `CALEON_HOST`) to
  learn its ID, so that part has to work anyway.

## Install on a NAS

Every push to `main` publishes the image at `ghcr.io/danibram/caleon` for `linux/amd64` and `linux/arm64`
(tags `latest`, `vX.Y.Z` for releases and `sha-<commit>`). The NAS only needs the compose file and a
`data/` folder next to it:

```sh
mkdir caleon && cd caleon
curl -O https://raw.githubusercontent.com/danibram/caleon/main/docker-compose.yml
docker compose up -d
# to update later
docker compose pull && docker compose up -d
```

To build the image yourself instead (a change of your own, or the SDK mode with the SDK copied to
`vendor/nabto4`, amd64 only):

```sh
docker build -t ghcr.io/danibram/caleon:latest .                   # --platform linux/arm64 for an ARM NAS
docker build --platform linux/amd64 --build-arg FEATURES=nabto-sdk -t ghcr.io/danibram/caleon:latest .
docker save ghcr.io/danibram/caleon:latest | gzip > caleon.tar.gz  # then docker load < caleon.tar.gz on the NAS
```

With the code on the NAS, `docker compose up -d --build` after uncommenting `build: .` in
`docker-compose.yml` does the same.

Without Docker, the [releases](https://github.com/danibram/caleon/releases) carry static binaries for Linux
(amd64, arm64) and macOS (Apple Silicon). Unpack, then `DATA_DIR=./data ./caleon`. Each release also tags
the image as `ghcr.io/danibram/caleon:X.Y.Z`.

The web UI is at `http://<nas-ip>:8088`. It has no user or password, so anyone on your network can change the
heating. If you expose it outside your home, put it behind an authenticating proxy or a VPN.

`network_mode: host` lets the server find the thermostat by broadcast. With a fixed IP (`CALEON_HOST` or in
Settings) it also works on a normal bridge network publishing port 8088.

Docker Desktop on macOS does not let containers reach the LAN over UDP (not even a minimal script), so on a Mac
the container cannot find the thermostat. A Linux NAS has no such problem.

Weather settings and encrypted API keys live in SQLite. The master key stays in a separate file. See [credential storage and Docker secrets](docs/weather-credentials.md) for setup and backup instructions.

### Configuration (environment variables)

| Variable | Default | |
|---|---|---|
| `BIND` | `0.0.0.0:8088` | address and port of the web UI |
| `DATA_DIR` | `/data` in Docker | `settings.json`, `history.sqlite`, optional `secrets.key` and `backups/` |
| `CALEON_SECRETS_KEY_FILE` | `DATA_DIR/secrets.key` | external 32-byte binary master key file for encrypted provider credentials; must exist when set |
| `CALEON_HOST` | discovery | fixed IP of the thermostat, for example `192.168.1.50` |
| `CALEON_DEVICE` | first to answer | device ID when you have several, for example `0123456789ab.sorel-connect.net` |
| `TRANSPORT` | `udp` | initial mode (`udp`, `session` or `sdk`); afterwards the choice in Settings wins |
| `CALEON_EMAIL` | (empty) | authorized e-mail for the `session` mode |
| `POLL_SECONDS` | `60` | how often the device is read and a sample stored |
| `BACKUP_HOURS` | `24` | how often the configuration is checked; a backup is written only when something changed. `0` disables it |
| `BACKUP_KEEP` | `30` | automatic backups to keep |
| `PROMETHEUS` | `1` | `/metrics` in Prometheus format; `0` turns it off |
| `WEB` | `1` | `0` serves only the API and `/metrics`, no web UI |
| `HISTORY` | `1` | `0` stores no samples in SQLite (charts stay empty) |
| `MQTT_URL` | (empty) | `mqtt://user:password@host:1883` turns on the Home Assistant integration |
| `MQTT_TOPIC` | `caleon` | base of the MQTT topics |
| `MQTT_DISCOVERY_PREFIX` | `homeassistant` | Home Assistant's discovery prefix |
| `READ_ONLY` | (empty) | with `1`, the web shows everything but the server refuses every change to the thermostat (for a display-only screen or for testing) |

With `WEB=0`, `HISTORY=0` and `BACKUP_HOURS=0` the server is a plain service that reads the thermostat once a
minute and feeds Prometheus and Home Assistant. `contrib/stack/docker-compose.yml` runs it that way next to
Mosquitto, Home Assistant, Prometheus and Grafana, and `contrib/stack/test.sh` checks the whole chain.

Reads (rooms, status screens) do not wake the thermostat's screen. Moving through its menu does, so the
screen lights up while someone uses Experto and during the backup walk (about a minute).

## Backups

Each backup is a JSON file with:

- rooms: name, type, state and the weekly heating and cooling schedules;
- the global control and the hot water schedule, when present;
- which zone heats each room;
- the whole expert menu tree with the value of every screen.

To read the menu, the server walks it as a person would on the screen. It only enters submenus and goes back.
It never presses yes/no, +/− or selections, and it does not enter Factory values, System update or Select
network (that one starts a WiFi scan). It takes about a minute.

A restore writes room names and schedules. Expert menu settings are only compared, because the device does not
allow writing them directly. They are changed step by step with +/− from Expert, and the comparison tells you
which screens differ.

## Home Assistant

With `MQTT_URL` set, the server talks to an MQTT broker (the Mosquitto add-on, or the one in
`contrib/stack`) and publishes Home Assistant discovery messages, so HA creates everything on its own under
one "Caleon" device: a `climate` entity per room (current temperature, target in 0.5 °C steps, `heat`/`off`,
presets `eco` and `comfort`; no preset is Normal), temperature and humidity sensors, a "heating" sensor per
room, the circuit pump, the boiler demand, a "Toda la casa" select for the whole-house mode and the time of
the last read. State is retained and refreshed after every read; changes made in HA go back to the
thermostat and are read back for confirmation. The thermostat only moves the setpoint in half-degree steps,
so a new target is sent as a series of steps.

Topics under `MQTT_TOPIC`: `room/<id>/{mode,preset,temperature,humidity,target,heating,action}`,
`room/<id>/set/{mode,preset,temperature}`, `house/mode`, `house/set/mode`, `pump`, `boiler`, `last_read`
and `status` (`online`/`offline`, set by the broker when the server drops). Anything else that speaks MQTT
(Node-RED, openHAB) can use the same topics. `READ_ONLY=1` ignores commands.

## Prometheus and Grafana

The server exposes its last read at `/metrics` in Prometheus text format (`PROMETHEUS=0` turns it off).
Everything is a gauge:

| Metric | Labels | |
|---|---|---|
| `caleon_room_temperature_celsius` | `id`, `room`, `type` | room temperature |
| `caleon_room_humidity_percent` | `id`, `room`, `type` | rooms with a humidity sensor |
| `caleon_room_setpoint_celsius` | `id`, `room`, `type` | absent while the room is off |
| `caleon_room_mode` | `id`, `room`, `type`, `mode` | 1 for the active mode (`off`, `regular`, `eco`, `turbo`), 0 for the others |
| `caleon_room_heating` | `id`, `room`, `type` | 1 while a zone valve of the room is open |
| `caleon_output_on` | `id`, `room`, `type`, `zone`, `output` | each zone valve |
| `caleon_status` | `key`, `group`, `label`, `unit` | every numeric value of the status screens: sensors, pump, boiler demand |
| `caleon_up`, `caleon_last_read_timestamp_seconds` | | whether the last read worked, and when |

The device is read once a minute (`POLL_SECONDS`), so a shorter scrape interval only repeats values:

```yaml
scrape_configs:
  - job_name: caleon
    scrape_interval: 60s
    static_configs:
      - targets: ["<nas-ip>:8088"]
```

`contrib/grafana/caleon.json` is a dashboard to import in Grafana: temperature and setpoint per room, humidity,
the mode of each room over time, when each room heated, pump, valves and boiler demand, and the device
sensors. It asks for your Prometheus data source on import.

## Repository layout

A Cargo workspace plus the web package:

- `crates/caleon`: the client library. Discovery, the UDP and session transports (and the SDK one behind the
  `nabto-sdk` feature), rooms, schedules, the expert menu and the status screen parser. No server and no
  async runtime.
- `crates/caleon-server`: the `caleon` binary. HTTP API, SQLite history, poller, backups and the embedded
  web UI.
- `web/`: the React UI (pnpm). `cargo test export_types` writes its API types from the server structs.
- `static/icons`: the device's SVG icons, served by the server and by the Vite mock.

To talk to the thermostat from another Rust project, depend on the library alone:

```toml
[dependencies]
caleon = { git = "https://github.com/danibram/caleon" }   # or path = "../caleon/crates/caleon"
```

```rust
use std::sync::Arc;
use caleon::device::{Device, RoomChange};
use caleon::transport::UdpTransport;

let found = caleon::locate("", "")?; // broadcast discovery, or ("192.168.1.50", "")
let transport = UdpTransport::new(found.addr, Arc::new(caleon::schema()?))?;
let device = Device::new(found.id, Arc::new(transport));
for (id, room) in device.room_list()?.iter().enumerate() {
    let state = device.room_state(id as u16, &RoomChange::default())?;
    println!("{}: {} ({})", room["name"], room["temperature"], state.mode);
}
```

Reads can be repeated freely; `tdelta` and menu actions are relative and are sent once. The `ts` feature
adds `ts-rs` derives to the public structs and `nabto-sdk` adds the SDK transport.

## Development

The web UI is React (`web/`). The Rust binary serves an embedded `web/dist`; during development use the Vite
proxy instead.

```sh
cargo test
# terminal 1: API (READ_ONLY=1 so nothing is written to the thermostat)
READ_ONLY=1 DATA_DIR=./data BIND=127.0.0.1:8088 cargo run --release
# terminal 2: UI with hot reload (proxies /api and /icons to port 8088)
cd web && pnpm install && pnpm dev
```

`CALEON_API=http://127.0.0.1:8089 pnpm dev` points the proxy at another server. `VITE_MOCK=1 pnpm dev` starts
the UI with no server and no thermostat. It answers with the captured data in `web/src/mocks`, with timestamps
shifted to now, and rejects changes like a read-only server.

To bundle the UI into the binary: `cd web && pnpm build`, then `cargo build --release`.

`cargo test export_types` writes the TypeScript types of the Rust structs to `web/src/api/generated/`.

On macOS it runs natively. The first time, the system asks for the "Local network" permission for the binary;
without it the thermostat is not found. To try the SDK mode, build for Intel and run under Rosetta:
`cargo run -p caleon-server --target x86_64-apple-darwin --features nabto-sdk`.

CI (`.github/workflows/ci.yml`) runs `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`,
`pnpm lint` and `pnpm build`, and checks the Docker build on pull requests. `docker.yml` publishes the
multi-arch image to GitHub Container Registry on every push to `main` and on `v*` tags. `release.yml` turns
a `vX.Y.Z` tag (matching the version in `Cargo.toml`) into a GitHub release with the binaries and generated
notes.

## API

| Method | Path | |
|---|---|---|
| GET/POST | `/api/settings` | connection mode and IP: `{"transport": "udp", "host": ""}` |
| GET | `/api/state` | full state: rooms, global control, device status, zones, features |
| POST | `/api/refresh` | forces a read |
| POST | `/api/rooms/{id}` | `{"tdelta": 1}`, `{"mode": "eco"}`, `{"name": "Salón"}`, `{"season": "cooling"}`, `{"vacation": "2026-11-01 12:00"}` or `{"vacation": null}` |
| GET | `/api/rooms/{id}/timer?type=0` | weekly schedule (0 heating, 1 cooling; id 65535 is hot water) |
| PUT | `/api/rooms/{id}/timer/{day}` | `{"slots": [48 modes]}`, day 0 = Monday |
| POST | `/api/rooms/{id}/timer/{day}/copy` | `{"mask": 31}`, bit 0 = Monday |
| POST | `/api/menu/action` | `{"id": "cal:1", "action": "goto:18"}`: an expert menu action, returns the new screen |
| GET | `/api/menu/detail?id=&type=` | rereads a screen without moving the menu |
| POST | `/api/menu/set` | date, time, text or timer of a screen |
| POST | `/api/menu/open` | `{"path": ["Ajustes", "Zonas"]}`: opens the menu at that path of titles (only follows entries that open screens) |
| GET | `/api/menu/values` | the value of every menu screen, from the latest backup |
| GET | `/api/history?hours=24` | history per room |
| GET | `/api/metrics?hours=24` | history of every status value |
| GET | `/api/log` | device notices |
| GET | `/api/events?limit=300` | what the server did, newest first: `{ts, source: web/ha/auto/server, room, text}` |
| GET/POST | `/api/backups` | list / create a backup |
| GET/DELETE | `/api/backups/{name}` | download / delete |
| POST | `/api/backups/{name}/restore` | `{"apply": false}` compares; `{"apply": true}` restores; accepts `"backup": {...}` for an uploaded file |

Room IDs start at 0. 65535 is "all rooms". After every change the server rereads the device a few times in
the background, so valves and pump show up as soon as they react.

## Protocol details

- UDP packet: `<hdr u32> <seq u32> <0 u32> <query id u32> <parameters>`, all big-endian. The answer repeats
  the header with bit `0x00800000` set (and `0x00400000` when the device rejected the query). Parameters go in
  the order of the XML: integers at their size, `raw` as a u16 length plus the bytes, lists as a u16 count plus
  each element.
- Discovery: `<1> <seq> <0> "*\0"` broadcast to port 5570. Each device answers with its IP, port and ID.
- Reads are retried. Relative changes (setpoint ±1 and menu actions) are sent once, so a lost answer cannot
  apply them twice.
- `room_state.json?id=N&tdelta=0&season=&vacation=&mode=&name=` reads a room. Empty text parameters mean "leave
  unchanged"; when set, they are NUL-terminated. `tdelta=±1` moves the setpoint by 0.5 °C.
- `room_timer.json`: 21 32-bit integers, 3 per day, 2 bits per half hour (0 off, 1 normal, 2 eco, 3 comfort).
  `day=-1` only reads.
- The expert menu is `action_2.json?id=<screen>&action=<action>` (`goto:N`, `bit:N`, `sel:0x…`, `plus`,
  `minus`, `back`, `forward`, `exit`, `status`). Each screen type has its own detail query
  (`string_list_2.json`, `dynamic_list_2.json`, `options.json`, `information.json`...). Those queries read by id
  without moving the menu cursor, so the server reads the status screen (`cal:9`, `cal:9.1`) every minute
  without getting in the way.
- Session mode: `U_CONNECT` with an IPX payload and a CP_ID (the e-mail). The device answers with NOTIFY
  connect-ok (0x0001) and a session id; an unauthorized e-mail gets 0x8004. Each RPC goes in a `DATA` packet
  with a CRYPT payload of suite 0x000A (no encryption), 1-2 padding bytes and a 16-bit checksum at the end
  (counted in the payload length). Sequence in bytes 12-13, about 150 RPCs per session with 250 ms between
  sends.
- With the SDK mode on Linux, `crates/caleon/build.rs` forces the `bfd` linker, because the SDK was built with GCC 4.9 and registers
  constructors in `.ctors` sections, which `lld`, rustc's default linker, does not run.

## License

MIT, see [LICENSE](LICENSE). `crates/caleon/unabto_queries.xml` is the °CALEON's RPC interface definition, the same one the
official app carries; without it the server cannot encode the queries. The room and device icons in
`static/icons` are SOREL's artwork from the official app, kept so rooms look the same as in the app; they
remain SOREL's property. SOREL, °CALEON and Nabto are trademarks of their owners. The data in
`web/src/mocks` is synthetic.

# caleon

[English](README.md)

Servidor en Rust para monitorizar y controlar un termostato SOREL °CALEON desde la red local, sin la app ni la
nube de Sorel. Pensado para correr en un NAS con Docker. Es Rust puro y no necesita el SDK de Nabto.

> Proyecto independiente, sin relación con SOREL ni con Nabto. El protocolo viene de observar cómo habla la app
> oficial con el equipo. Escribe en tu calefacción, así que úsalo bajo tu responsabilidad.

La web tiene lo mismo que la app oficial (°CALEON 7.0.36) y algo más. Está pensada sobre todo para el móvil
(barra de pestañas abajo, tema claro y oscuro, transiciones entre pantallas) y funciona igual en el ordenador:

<p align="center">
  <img src="docs/screenshots/home.png" width="180" alt="Inicio: habitaciones, modos y temperaturas de hoy">
  <img src="docs/screenshots/room.png" width="180" alt="Una habitación: indicador, consigna y programa">
  <img src="docs/screenshots/schedule.png" width="180" alt="Editor de horarios semanal">
  <img src="docs/screenshots/charts.png" width="180" alt="Gráficas">
</p>
<p align="center"><img src="docs/screenshots/desktop.png" width="720" alt="Inicio en el ordenador"></p>

- Inicio: habitaciones con temperatura, humedad, consigna (+/−), modo (Apagado, Normal, Eco, Confort, con la
  cuenta atrás de Confort) y control global de todas las habitaciones. Marca "Calentando" cuando el relé de la zona
  de esa habitación está activo.
- Habitación: horarios, vacaciones (activar y terminar), cambio de estación si el equipo lo permite, nombre,
  avisos, experto, estado y su histórico. La temperatura de cada modo (Normal, Eco, Confort) se edita en un
  solo sitio: el termostato guarda una consigna por modo y solo mueve la del modo activo, así que el servidor
  pasa un momento por cada modo y vuelve al que tenía la habitación.
- Horarios: los 7 días en franjas de media hora para calefacción y refrigeración, pintando con el ratón o el dedo,
  y "copiar a día siguiente / lunes-viernes / sábado-domingo / lunes-domingo".
- Experto: el menú completo del equipo (idioma, fecha y hora, dispositivos, habitaciones, funciones °Cbox, zonas,
  WiFi, valores de servicio, bloqueo, valores de fábrica...). Es el mismo menú remoto que pinta la app. El
  °CALEON manda cada pantalla y la web la dibuja.
- Estado: entradas (sensores 1-Wire, humedad, PWM), salidas (bomba y relés de zona, con la habitación que
  calienta cada zona) y bomba de circuito. Entradas y salidas llevan el nombre del equipo CAN al que
  pertenecen (°CALEONbox o el propio termostato).
- Gráficas: temperatura y consigna por habitación, humedad, todos los sensores del equipo y una línea de tiempo
  de cuándo estuvo encendido cada relé. 6 h, 24 h, 7 d y 30 d. Los sensores del propio termostato muestran
  décimas, pero su precisión es de ±1 °C y ±6 % de humedad.
- Copias: copia de toda la configuración en JSON (manual, o automática cada día solo si algo cambió), descarga,
  subida, comparación con el equipo y restauración de nombres y horarios.
- Avisos: registro de mensajes del equipo, firmware y funciones activas.
- Ajustes: modo de conexión (directo, con sesión o SDK) e IP del termostato.

## Cómo habla con el termostato

El °CALEON usa Nabto 4 (uNabto). Además del protocolo cifrado que usa la app por la nube, uNabto trae activado
por defecto un protocolo local (`NABTO_ENABLE_LOCAL_ACCESS_LEGACY_PROTOCOL`): cada consulta RPC es un paquete
UDP al puerto 5570, sin sesión ni cifrado. El servidor usa ese protocolo, escrito en Rust puro
(`src/transport.rs`), y codifica cada consulta según `unabto_queries.xml` (`src/schema.rs`). Las respuestas
salen con la misma forma JSON que da el SDK oficial.

No depende de ninguna librería cerrada, así que funciona en cualquier arquitectura: NAS x86_64 o ARM, Raspberry Pi
y Mac en nativo.

En Ajustes se elige el modo de conexión:

- **Directo por la red local** (por defecto): el protocolo UDP de arriba, sin control de acceso.
- **Con sesión y correo**: la conexión local de uNabto con sesión (`U_CONNECT` + paquetes `DATA` con cifrado
  nulo). Igual que el modo directo pero con control de acceso, así que necesita un correo que esté autorizado en el
  equipo (Experto › Ajustes › WiFi › Control de acceso, hasta cinco). Es un respaldo por si el modo directo dejara de funcionar.
- **SDK oficial de Nabto**: la librería cerrada de Nabto 4.3, solo para x86_64. Solo aparece si el binario se
  compiló con `--features nabto-sdk` (ver abajo).

El cambio solo se aplica si el termostato responde con el modo nuevo, y se guarda en `DATA_DIR/settings.json`.

Cualquier equipo de tu red puede hablar con el termostato de esta forma, sin contraseña. Así viene de fábrica,
no lo abre este servidor; tenlo en cuenta si tu red tiene invitados.

### SDK de Nabto (opcional)

Modo de respaldo que habla con el equipo a través de la librería cliente oficial de Nabto 4.3 en vez del
protocolo UDP en Rust. En las pruebas, una copia completa hecha con el SDK y otra con el modo directo salieron
idénticas, así que solo tiene sentido si el modo directo dejara de funcionar o para comparar resultados.

**Conseguir la librería.** Es software cerrado y no está en este repositorio. Nabto la distribuye dentro de su
wrapper de Python, [nabto/nabto4-python-wrapper](https://github.com/nabto/nabto4-python-wrapper): las carpetas
`extension/linux64/lib` y `extension/mac64/lib` traen `libnabto_client_api_static.a` y
`libnabto_static_external.a`. Cópialas con esta estructura, o apunta `NABTO_SDK_DIR` a una carpeta que la
tenga:

```
vendor/nabto4/linux64/lib/libnabto_client_api_static.a
vendor/nabto4/linux64/lib/libnabto_static_external.a
vendor/nabto4/mac64/lib/libnabto_client_api_static.a
vendor/nabto4/mac64/lib/libnabto_static_external.a
```

`vendor/` está en `.gitignore`. No hace falta la cabecera, porque las funciones que se usan están declaradas en
`src/nabto.rs`. Las condiciones de uso de esas librerías son las de Nabto, no las de este proyecto.

**Compilar.**

```sh
# Linux x86_64 (necesita GNU ld, paquete binutils)
cargo build --release -p caleon-server --features nabto-sdk

# macOS: la librería es solo para Intel; en Apple Silicon corre bajo Rosetta
rustup target add x86_64-apple-darwin
cargo build --release -p caleon-server --target x86_64-apple-darwin --features nabto-sdk

# Docker (solo amd64), con vendor/nabto4 ya copiado al lado del Dockerfile
docker build --platform linux/amd64 --build-arg FEATURES=nabto-sdk -t caleon:latest .
```

Si falta la librería, `build.rs` para la compilación diciendo en qué ruta la buscaba.

**Usar.** Con un binario compilado así, en Ajustes aparece el modo "SDK oficial de Nabto"; también se puede
arrancar con `TRANSPORT=sdk`. La primera vez el SDK crea un perfil autofirmado (`caleon-nas`) en
`DATA_DIR/nabto/` y lo reutiliza después. Las lecturas se reintentan hasta tres veces; los cambios relativos
(consigna, acciones del menú) se envían una sola vez, igual que en los otros modos.

**Limitaciones.**

- Solo x86_64. Nabto publica la librería para Linux y macOS de 64 bits Intel, así que no hay modo SDK en un NAS ARM,
  en una Raspberry Pi ni en Apple Silicon en nativo. Los modos directo y con sesión no tienen este límite.
- En Linux hay que enlazar con GNU ld (`build.rs` añade `-fuse-ld=bfd`), porque el SDK se compiló con GCC 4.9 y
  registra constructores en secciones `.ctors` que `lld`, el enlazador por defecto de rustc, no ejecuta.
- Es código cerrado, así que no se puede auditar ni corregir.
- La integración continua no lo compila porque no tiene la librería, así que un cambio puede romperlo sin que
  se note hasta que alguien lo compile a mano.
- Al parar el servidor, el proceso sale sin llamar a `nabtoShutdown`, que puede quedarse bloqueado mucho
  tiempo. El perfil ya está en disco, así que no se pierde nada.
- El SDK solo sustituye las consultas. El servidor sigue encontrando el equipo por descubrimiento UDP (o por
  `CALEON_HOST`) para saber su ID, así que esa parte tiene que funcionar igualmente.

## Instalar en el NAS

Cada push a `main` publica la imagen en `ghcr.io/danibram/caleon` para `linux/amd64` y `linux/arm64`
(etiquetas `latest`, `vX.Y.Z` en las versiones y `sha-<commit>`). En el NAS solo hacen falta el archivo
compose y una carpeta `data/` al lado:

```sh
mkdir caleon && cd caleon
curl -O https://raw.githubusercontent.com/danibram/caleon/main/docker-compose.yml
docker compose up -d
# para actualizar más adelante
docker compose pull && docker compose up -d
```

Para construir la imagen tú mismo (un cambio propio, o el modo SDK con el SDK copiado en `vendor/nabto4`,
solo amd64):

```sh
docker build -t ghcr.io/danibram/caleon:latest .                   # --platform linux/arm64 para un NAS ARM
docker build --platform linux/amd64 --build-arg FEATURES=nabto-sdk -t ghcr.io/danibram/caleon:latest .
docker save ghcr.io/danibram/caleon:latest | gzip > caleon.tar.gz  # y docker load < caleon.tar.gz en el NAS
```

Con el código en el NAS, `docker compose up -d --build` hace lo mismo tras descomentar `build: .` en
`docker-compose.yml`.

Sin Docker, las [releases](https://github.com/danibram/caleon/releases) traen binarios estáticos para Linux
(amd64, arm64) y macOS (Apple Silicon). Descomprime y ejecuta `DATA_DIR=./data ./caleon`. Cada release etiqueta
también la imagen como `ghcr.io/danibram/caleon:X.Y.Z`.

La web queda en `http://<ip-del-nas>:8088`. No tiene usuario ni contraseña, así que cualquiera en tu red puede
cambiar la calefacción. Si la expones fuera de casa, ponla detrás de un proxy con autenticación o de una VPN.

`network_mode: host` permite encontrar el termostato por broadcast. Si fijas su IP (`CALEON_HOST` o en
Ajustes), también funciona con una red bridge normal publicando el puerto 8088.

Docker Desktop en macOS no deja que los contenedores lleguen a la LAN por UDP (ni siquiera un script mínimo), así
que en el Mac el contenedor no encuentra el termostato. En un NAS Linux no pasa.

La configuración meteorológica y las API keys cifradas se guardan en SQLite. La clave maestra queda en un archivo separado. Consulta [credenciales y secretos de Docker](docs/weather-credentials.md) para la configuración y las copias.

### Configuración (variables de entorno)

| Variable | Por defecto | |
|---|---|---|
| `BIND` | `0.0.0.0:8088` | dirección y puerto de la web |
| `DATA_DIR` | `/data` en Docker | `settings.json`, `history.sqlite`, `secrets.key` opcional y `backups/` |
| `CALEON_SECRETS_KEY_FILE` | `DATA_DIR/secrets.key` | archivo externo de clave maestra de 32 bytes binarios; debe existir si se indica |
| `CALEON_HOST` | descubrimiento | IP fija del termostato, por ejemplo `192.168.1.50` |
| `CALEON_DEVICE` | el primero que responda | ID del equipo si tienes varios, por ejemplo `0123456789ab.sorel-connect.net` |
| `TRANSPORT` | `udp` | modo inicial (`udp`, `session` o `sdk`); después manda lo elegido en Ajustes |
| `CALEON_EMAIL` | (vacío) | correo autorizado para el modo `session` |
| `POLL_SECONDS` | `60` | cada cuánto se lee el equipo y se guarda una muestra |
| `BACKUP_HOURS` | `24` | cada cuánto se revisa la configuración; solo se guarda copia si cambió algo. `0` lo desactiva |
| `BACKUP_KEEP` | `30` | copias automáticas que se conservan |
| `PROMETHEUS` | `1` | `/metrics` en formato Prometheus; `0` lo desactiva |
| `WEB` | `1` | con `0` solo sirve la API y `/metrics`, sin interfaz web |
| `HISTORY` | `1` | con `0` no guarda muestras en SQLite (las gráficas quedan vacías) |
| `MQTT_URL` | (vacío) | `mqtt://usuario:clave@host:1883` activa la integración con Home Assistant |
| `MQTT_TOPIC` | `caleon` | base de los topics MQTT |
| `MQTT_DISCOVERY_PREFIX` | `homeassistant` | prefijo de descubrimiento de Home Assistant |
| `READ_ONLY` | (vacío) | con `1`, la web lo enseña todo pero el servidor rechaza cualquier cambio en el termostato (sirve para una pantalla de consulta o para hacer pruebas) |

Con `WEB=0`, `HISTORY=0` y `BACKUP_HOURS=0` el servidor se queda en un servicio que lee el termostato una vez
por minuto y alimenta Prometheus y Home Assistant. `contrib/stack/docker-compose.yml` lo arranca así junto a
Mosquitto, Home Assistant, Prometheus y Grafana, y `contrib/stack/test.sh` comprueba toda la cadena.

Las lecturas (habitaciones, pantallas de estado) no despiertan la pantalla del termostato. Moverse por su menú
sí, así que la pantalla se enciende mientras alguien usa Experto y durante el recorrido de la copia (un minuto).

## Copias de seguridad

Cada copia es un JSON con:

- habitaciones: nombre, tipo, estado y horarios semanales de calefacción y refrigeración;
- control global y horario de ACS si existe;
- qué zona calienta cada habitación;
- el árbol completo del menú experto con el valor de cada pantalla.

Para leer el menú, el servidor lo recorre como lo haría una persona en la pantalla. Solo entra en submenús y
vuelve atrás. Nunca pulsa sí/no, +/− ni selecciones, y no entra en Valores de fábrica, Actualización del sistema
ni Seleccionar red (lanza un escaneo WiFi). Tarda alrededor de un minuto.

La restauración escribe nombres de habitación y horarios. Los ajustes del menú experto solo se comparan, porque el
equipo no permite escribirlos directamente. Se cambian paso a paso con +/− desde Experto, y la comparación te dice
qué pantallas difieren.

## Home Assistant

Con `MQTT_URL` definido, el servidor se conecta a un broker MQTT (el add-on Mosquitto, o el de
`contrib/stack`) y publica los mensajes de descubrimiento de Home Assistant, así que HA lo crea todo solo bajo
un dispositivo "Caleon": una entidad `climate` por habitación (temperatura actual, objetivo en pasos de
0,5 °C, `heat`/`off`, presets `eco` y `comfort`; sin preset es Normal), sensores de temperatura y humedad, un
sensor "calentando" por habitación, la bomba del circuito, la demanda a la caldera, un selector "Toda la casa"
para el modo global y la hora de la última lectura. El estado queda retenido y se renueva tras cada lectura;
los cambios hechos en HA van al termostato y se releen para confirmarlos. El termostato solo mueve la consigna
de medio en medio grado, así que un objetivo nuevo se envía como una serie de pasos.

Topics bajo `MQTT_TOPIC`: `room/<id>/{mode,preset,temperature,humidity,target,heating,action}`,
`room/<id>/set/{mode,preset,temperature}`, `house/mode`, `house/set/mode`, `pump`, `boiler`, `last_read` y
`status` (`online`/`offline`, que pone el broker si el servidor cae). Cualquier otra cosa que hable MQTT
(Node-RED, openHAB) puede usar los mismos topics. Con `READ_ONLY=1` se ignoran los comandos.

## Prometheus y Grafana

El servidor expone su última lectura en `/metrics` en formato de texto de Prometheus (`PROMETHEUS=0` lo
desactiva). Todo son gauges:

| Métrica | Etiquetas | |
|---|---|---|
| `caleon_room_temperature_celsius` | `id`, `room`, `type` | temperatura de la habitación |
| `caleon_room_humidity_percent` | `id`, `room`, `type` | habitaciones con sensor de humedad |
| `caleon_room_setpoint_celsius` | `id`, `room`, `type` | no aparece mientras la habitación está apagada |
| `caleon_room_mode` | `id`, `room`, `type`, `mode` | 1 en el modo activo (`off`, `regular`, `eco`, `turbo`), 0 en los demás |
| `caleon_room_heating` | `id`, `room`, `type` | 1 mientras una válvula de zona de la habitación está abierta |
| `caleon_output_on` | `id`, `room`, `type`, `zone`, `output` | cada válvula de zona |
| `caleon_status` | `key`, `group`, `label`, `unit` | todos los valores numéricos de las pantallas de estado: sensores, bomba, demanda a la caldera |
| `caleon_up`, `caleon_last_read_timestamp_seconds` | | si la última lectura funcionó, y cuándo |

El equipo se lee una vez por minuto (`POLL_SECONDS`), así que un intervalo de scrape más corto solo repite
valores:

```yaml
scrape_configs:
  - job_name: caleon
    scrape_interval: 60s
    static_configs:
      - targets: ["<ip-del-nas>:8088"]
```

`contrib/grafana/caleon.json` es un panel para importar en Grafana: temperatura y objetivo por habitación,
humedad, el modo de cada habitación en el tiempo, cuándo ha calentado cada una, bomba, válvulas y demanda a la
caldera, y los sensores del equipo. Al importarlo pide tu fuente de datos de Prometheus.

## Estructura del repositorio

Un workspace de Cargo más el paquete web:

- `crates/caleon`: la librería cliente. Descubrimiento, los transportes UDP y con sesión (y el del SDK tras
  la feature `nabto-sdk`), habitaciones, horarios, menú experto y el parser de las pantallas de estado. Sin
  servidor ni runtime asíncrono.
- `crates/caleon-server`: el binario `caleon`. API HTTP, histórico en SQLite, poller, copias y la web
  embebida.
- `web/`: la interfaz React (pnpm). `cargo test export_types` escribe sus tipos de la API a partir de los
  structs del servidor.
- `static/icons`: los iconos SVG del equipo, que sirven el servidor y el mock de Vite.

Para hablar con el termostato desde otro proyecto en Rust basta con la librería:

```toml
[dependencies]
caleon = { git = "https://github.com/danibram/caleon" }   # o path = "../caleon/crates/caleon"
```

```rust
use std::sync::Arc;
use caleon::device::{Device, RoomChange};
use caleon::transport::UdpTransport;

let found = caleon::locate("", "")?; // descubrimiento por broadcast, o ("192.168.1.50", "")
let transport = UdpTransport::new(found.addr, Arc::new(caleon::schema()?))?;
let device = Device::new(found.id, Arc::new(transport));
for (id, room) in device.room_list()?.iter().enumerate() {
    let state = device.room_state(id as u16, &RoomChange::default())?;
    println!("{}: {} ({})", room["name"], room["temperature"], state.mode);
}
```

Las lecturas se pueden repetir sin problema; `tdelta` y las acciones del menú son relativas y se envían una
sola vez. La feature `ts` añade las derivaciones de `ts-rs` a los structs públicos y `nabto-sdk` añade el
transporte del SDK.

## Desarrollo

Interfaz web en React (`web/`). El binario de Rust sirve `web/dist` embebido; en desarrollo conviene el proxy de
Vite.

```sh
cargo test
# terminal 1: API (READ_ONLY=1 para no escribir en el termostato)
READ_ONLY=1 DATA_DIR=./data BIND=127.0.0.1:8088 cargo run --release
# terminal 2: UI con recarga en caliente (proxy /api y /icons al puerto 8088)
cd web && pnpm install && pnpm dev
```

`CALEON_API=http://127.0.0.1:8089 pnpm dev` apunta el proxy a otro servidor. `VITE_MOCK=1 pnpm dev` arranca
la UI sin servidor ni termostato. Responde con los datos capturados en `web/src/mocks`, con las horas
movidas al momento actual, y rechaza los cambios como un servidor en solo lectura.

Para empaquetar la UI en el binario: `cd web && pnpm build`, luego `cargo build --release`.

`cargo test export_types` escribe los tipos TypeScript de los structs de Rust en `web/src/api/generated/`.

En macOS corre en nativo. La primera vez el sistema pide el permiso de "Red local" para el binario; sin él no
encuentra el termostato. Para probar el modo SDK hay que compilar para Intel y correr bajo Rosetta:
`cargo run -p caleon-server --target x86_64-apple-darwin --features nabto-sdk`.

La integración continua (`.github/workflows/ci.yml`) pasa `cargo fmt --check`, `cargo clippy -D warnings`,
`cargo test`, `pnpm lint` y `pnpm build`, y comprueba la build de Docker en los pull requests. `docker.yml`
publica la imagen multiarquitectura en GitHub Container Registry en cada push a `main` y en las etiquetas
`v*`. `release.yml` convierte una etiqueta `vX.Y.Z` (igual que la versión de `Cargo.toml`) en una release de
GitHub con los binarios y notas generadas.

## API

| Método | Ruta | |
|---|---|---|
| GET/POST | `/api/settings` | modo de conexión e IP: `{"transport": "udp", "host": ""}` |
| GET | `/api/state` | estado completo: habitaciones, control global, estado del equipo, zonas, funciones |
| POST | `/api/refresh` | fuerza una lectura |
| POST | `/api/rooms/{id}` | `{"tdelta": 1}`, `{"mode": "eco"}`, `{"name": "Salón"}`, `{"season": "cooling"}`, `{"vacation": "2026-11-01 12:00"}` o `{"vacation": null}` |
| GET | `/api/rooms/{id}/timer?type=0` | horario semanal (0 calefacción, 1 refrigeración; el id 65535 es ACS) |
| PUT | `/api/rooms/{id}/timer/{día}` | `{"slots": [48 modos]}`, día 0 = lunes |
| POST | `/api/rooms/{id}/timer/{día}/copy` | `{"mask": 31}`, bit 0 = lunes |
| POST | `/api/menu/action` | `{"id": "cal:1", "action": "goto:18"}`: acción del menú experto, devuelve la nueva pantalla |
| GET | `/api/menu/detail?id=&type=` | relee una pantalla sin mover el menú |
| POST | `/api/menu/set` | fecha, hora, texto o temporizador de una pantalla |
| POST | `/api/menu/open` | `{"path": ["Ajustes", "Zonas"]}`: abre el menú en esa ruta de títulos (solo sigue entradas que abren pantallas) |
| GET | `/api/menu/values` | valor de cada pantalla del menú según la última copia |
| GET | `/api/history?hours=24` | histórico por habitación |
| GET | `/api/metrics?hours=24` | histórico de todos los valores del estado |
| GET | `/api/log` | avisos del equipo |
| GET | `/api/events?limit=300` | lo que ha hecho el servidor, lo más nuevo primero: `{ts, source: web/ha/auto/server, room, text}` |
| GET/POST | `/api/backups` | listar / crear copia |
| GET/DELETE | `/api/backups/{nombre}` | descargar / borrar |
| POST | `/api/backups/{nombre}/restore` | `{"apply": false}` compara; `{"apply": true}` restaura; admite `"backup": {...}` para una copia subida |

Los IDs de habitación empiezan en 0. El 65535 es "todas las habitaciones". Después de cada cambio el servidor
vuelve a leer el equipo unas cuantas veces en segundo plano, para que válvulas y bomba aparezcan en cuanto
reaccionan.

## Detalles del protocolo

- Paquete UDP: `<hdr u32> <seq u32> <0 u32> <id de consulta u32> <parámetros>`, todo big-endian. La respuesta
  repite la cabecera con el bit `0x00800000` (y `0x00400000` si el equipo rechazó la consulta). Los parámetros van
  en el orden del XML: enteros de su tamaño, `raw` como longitud u16 más los bytes, listas como número u16 de
  elementos más cada elemento.
- Descubrimiento: `<1> <seq> <0> "*\0"` por broadcast al puerto 5570. Cada equipo responde con su IP, puerto e ID.
- Las lecturas se reintentan. Los cambios relativos (consigna ±1 y acciones del menú) se envían una sola vez,
  para que una respuesta perdida no los aplique dos veces.
- `room_state.json?id=N&tdelta=0&season=&vacation=&mode=&name=` lee una habitación. Los parámetros de texto
  vacíos significan "no cambiar" y, si llevan valor, van terminados en `\0`. `tdelta=±1` mueve la consigna 0,5 °C.
- `room_timer.json`: 21 enteros de 32 bits, 3 por día, 2 bits por media hora (0 apagado, 1 normal, 2 eco,
  3 confort). `day=-1` solo lee.
- El menú experto es `action_2.json?id=<pantalla>&action=<acción>` (`goto:N`, `bit:N`, `sel:0x…`, `plus`,
  `minus`, `back`, `forward`, `exit`, `status`). Cada tipo de pantalla tiene su consulta de detalle
  (`string_list_2.json`, `dynamic_list_2.json`, `options.json`, `information.json`...). Esas consultas leen por id
  sin mover el cursor del menú, así que el servidor lee la pantalla de estado (`cal:9`, `cal:9.1`) cada minuto sin
  molestar.
- Modo con sesión: `U_CONNECT` con una carga IPX y una CP_ID (el correo). El equipo responde con NOTIFY
  connect-ok (0x0001) y un id de sesión; con un correo no autorizado devuelve 0x8004. Cada RPC va en un paquete
  `DATA` con una carga CRYPT de suite 0x000A (sin cifrar), 1-2 bytes de relleno y una suma de comprobación de 16
  bits al final (incluida en la longitud de la carga). Secuencia en los bytes 12-13, ~150 RPC por sesión con 250
  ms entre envíos.
- Con el modo SDK en Linux, `crates/caleon/build.rs` fuerza el enlazador `bfd`, porque el SDK se compiló con GCC 4.9 y registra
  constructores en secciones `.ctors` que `lld`, el enlazador por defecto de rustc, no ejecuta.

## Licencia

MIT, ver [LICENSE](LICENSE). `crates/caleon/unabto_queries.xml` es la definición de la interfaz RPC del °CALEON, la misma que
lleva la app oficial; sin ella el servidor no puede codificar las consultas. Los iconos de habitaciones y
dispositivos de `static/icons` son el arte de SOREL en la app oficial, conservados para que las habitaciones
se vean igual que en la app; siguen siendo propiedad de SOREL. SOREL, °CALEON y Nabto son marcas de sus
propietarios. Los datos de `web/src/mocks` son sintéticos.

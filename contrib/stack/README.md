# Example stack: Home Assistant, Prometheus and Grafana

`docker-compose.yml` runs caleon as a headless service (`WEB=0`, `HISTORY=0`) next to a Mosquitto
broker, Home Assistant, Prometheus and Grafana, with the Grafana data source and the °CALEON dashboard
already provisioned. Copy the folder to the NAS and:

```sh
docker compose up -d
```

- Home Assistant at `http://<nas-ip>:8123`. Create your user, then add the MQTT integration with broker
  `mosquitto`, port `1883`. The thermostat appears as a device with a climate entity per room, temperature
  and humidity sensors, "heating" sensors, the circuit pump, the boiler demand and the whole-house mode.
- Grafana at `http://<nas-ip>:3000` (admin / admin), dashboard "°CALEON".
- Prometheus at `http://<nas-ip>:9090`.

`test.sh` checks the whole chain. It brings the stack up in a fresh `./data`, onboards a throwaway Home
Assistant user, connects HA to the broker through the API and verifies the climate entities, the Prometheus
scrape and the Grafana dashboard. `./test.sh --down` removes everything afterwards. With `--no-caleon` it
skips the caleon container, for a server running outside Docker (`CALEON_URL`).

The broker accepts anonymous clients, which is fine on a home network; `mosquitto.conf` says how to add a
password. Docker Desktop on macOS cannot reach the thermostat over UDP from a container, so on a Mac run
caleon natively and the rest of the stack with `--no-caleon`.

#!/usr/bin/env bash
# End-to-end check of the example stack: brings it up, onboards a throwaway Home Assistant user,
# connects HA to the broker and verifies that the thermostat shows up everywhere.
#
#   ./test.sh            full stack, including the caleon container (needs the thermostat on the LAN)
#   ./test.sh --no-caleon  everything but caleon, for a caleon running outside Docker (CALEON_URL)
#   ./test.sh --down     stop and remove the stack and its data afterwards
#
# Needs curl, jq and docker compose. Runs in a fresh ./data, so it never touches a real install.
set -euo pipefail
cd "$(dirname "$0")"

HA=${HA_URL:-http://localhost:8123}
PROM=${PROM_URL:-http://localhost:9090}
GRAFANA=${GRAFANA_URL:-http://localhost:3000}
CALEON=${CALEON_URL:-http://localhost:8088}
BROKER=${MQTT_BROKER:-mosquitto}   # as Home Assistant reaches it (service name inside the stack)
COMPOSE=(docker compose)
if [[ -n "${COMPOSE_FILES:-}" ]]; then read -ra extra <<<"$COMPOSE_FILES"; COMPOSE+=("${extra[@]}"); fi
services=(mosquitto homeassistant prometheus grafana)
down=0
for a in "$@"; do
  case "$a" in
    --no-caleon) ;;
    --down) down=1 ;;
    *) echo "unknown option $a" >&2; exit 2 ;;
  esac
done
[[ " $* " == *" --no-caleon "* ]] || services+=(caleon)

fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "ok   $*"; }
wait_for() { # name, seconds, command...
  local name=$1 secs=$2; shift 2
  for ((i = 0; i < secs; i += 2)); do "$@" >/dev/null 2>&1 && return 0; sleep 2; done
  fail "$name did not come up in ${secs}s"
}

mkdir -p data
"${COMPOSE[@]}" up -d "${services[@]}"

# --- caleon
has_rooms() { curl -sf "$CALEON/api/state" | jq -e '.rooms | length > 0' >/dev/null; }
wait_for "caleon (first read of the thermostat)" 180 has_rooms
curl -sf "$CALEON/metrics" | grep -q '^caleon_room_temperature_celsius' || fail "/metrics has no room temperatures"
ok "caleon reads the thermostat and exposes /metrics"

# --- Prometheus scrapes it
wait_for prometheus 60 curl -sf "$PROM/-/ready"
prom_up() { curl -sf "$PROM/api/v1/query?query=caleon_up" | jq -e '.data.result[0].value[1] == "1"' >/dev/null; }
wait_for "prometheus scrape" 150 prom_up
ok "prometheus scrapes caleon_up == 1"

# --- Grafana has the dashboard and the data source
wait_for grafana 90 curl -sf "$GRAFANA/api/health"
curl -sf -u admin:admin "$GRAFANA/api/dashboards/uid/caleon" | jq -e '.dashboard.panels | length > 0' >/dev/null || fail "grafana dashboard missing"
curl -sf -u admin:admin "$GRAFANA/api/datasources/uid/prometheus" | jq -e '.type == "prometheus"' >/dev/null || fail "grafana data source missing"
ok "grafana has the °CALEON dashboard and the Prometheus data source"

# --- Home Assistant: throwaway owner, token, MQTT integration
wait_for "home assistant" 240 curl -sf "$HA/api/onboarding"
if curl -sf "$HA/api/onboarding" | jq -e '.[] | select(.step == "user") | .done == false' >/dev/null; then
  code=$(curl -sf -X POST "$HA/api/onboarding/users" -H 'content-type: application/json' \
    -d '{"client_id":"http://localhost:8123/","name":"caleon test","username":"caleon","password":"caleon-test-1234","language":"en"}' | jq -r .auth_code)
else
  # already onboarded (second run): log in with the same user
  flow=$(curl -sf -X POST "$HA/auth/login_flow" -H 'content-type: application/json' -d '{"client_id":"http://localhost:8123/","handler":["homeassistant",null],"redirect_uri":"http://localhost:8123/"}' | jq -r .flow_id)
  code=$(curl -sf -X POST "$HA/auth/login_flow/$flow" -H 'content-type: application/json' -d '{"client_id":"http://localhost:8123/","username":"caleon","password":"caleon-test-1234"}' | jq -r .result)
fi
token=$(curl -sf -X POST "$HA/auth/token" -d "grant_type=authorization_code&code=$code&client_id=http://localhost:8123/" | jq -r .access_token)
[[ -n "$token" && "$token" != null ]] || fail "could not get a Home Assistant token"
auth=(-H "authorization: Bearer $token" -H 'content-type: application/json')
# finish onboarding so the API stops redirecting
curl -s "${auth[@]}" -X POST "$HA/api/onboarding/core_config" >/dev/null || true
curl -s "${auth[@]}" -X POST "$HA/api/onboarding/analytics" >/dev/null || true
curl -s "${auth[@]}" -X POST "$HA/api/onboarding/integration" -d '{"client_id":"http://localhost:8123/","redirect_uri":"http://localhost:8123/"}' >/dev/null || true
ok "home assistant onboarded"

if ! curl -sf "${auth[@]}" "$HA/api/config/config_entries/entry" | jq -e '.[] | select(.domain == "mqtt")' >/dev/null; then
  # walk the MQTT config flow: pick "broker" when it offers a menu, fill in each form from its
  # schema (broker, port, protocol, the "other settings" section) so it survives new HA versions
  r=$(curl -s "${auth[@]}" -X POST "$HA/api/config/config_entries/flow" -d '{"handler":"mqtt","show_advanced_options":false}')
  for _ in 1 2 3 4 5 6; do
    type=$(jq -r .type <<<"$r"); flow=$(jq -r '.flow_id // ""' <<<"$r"); step=$(jq -r '.step_id // ""' <<<"$r")
    case "$type" in
      create_entry) break ;;
      menu) r=$(curl -s "${auth[@]}" -X POST "$HA/api/config/config_entries/flow/$flow" -d '{"next_step_id":"broker"}') ;;
      form)
        payload=$(jq -c --arg b "$BROKER" 'reduce (.data_schema[]?) as $f ({};
          if $f.name == "broker" then . + {broker: $b}
          elif $f.name == "port" then . + {port: 1883}
          elif $f.name == "protocol" then . + {protocol: "5"}
          elif ($f.schema | type) == "array" then . + {($f.name): (reduce ($f.schema[]) as $g ({};
            if $g.required != true then .
            elif $g.selector.boolean != null or $g.type == "boolean" then . + {($g.name): false}
            elif $g.selector.select != null then . + {($g.name): ($g.selector.select.options[0] | if type == "object" then .value else . end)}
            else . end))}
          else . end)' <<<"$r")
        r=$(curl -s "${auth[@]}" -X POST "$HA/api/config/config_entries/flow/$flow" -d "$payload") ;;
      abort) fail "mqtt flow aborted: $(jq -r .reason <<<"$r")" ;;
      *) fail "unexpected mqtt flow answer ($type, step $step): $r" ;;
    esac
  done
  [[ "$(jq -r .type <<<"$r")" == create_entry ]] || fail "mqtt integration not created: $r"
fi
ok "home assistant has the MQTT integration (broker $BROKER)"

# --- entities created by discovery
has_climate() { curl -sf "${auth[@]}" "$HA/api/states" | jq -e '[.[] | select(.entity_id | startswith("climate.caleon_"))] | length > 0' >/dev/null; }
wait_for "climate entities" 120 has_climate
states=$(curl -sf "${auth[@]}" "$HA/api/states")
rooms=$(jq -r '[.[] | select(.entity_id | startswith("climate.caleon_"))] | length' <<<"$states")
jq -r '.[] | select(.entity_id | startswith("climate.caleon_")) | "     \(.entity_id): \(.state), \(.attributes.current_temperature) °C, target \(.attributes.temperature), preset \(.attributes.preset_mode), \(.attributes.hvac_action)"' <<<"$states"
jq -e '.[] | select(.entity_id == "sensor.caleon_ultima_lectura") | .state | test("^20")' <<<"$states" >/dev/null || fail "sensor.caleon_last_read missing"
ok "home assistant shows $rooms climate entities, sensors and the last read"

if (( down )); then
  "${COMPOSE[@]}" down -v
  rm -rf data
  ok "stack removed"
fi
echo "ALL OK"

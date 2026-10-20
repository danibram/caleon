use super::{
    http, ApiKey, Coordinates, ProviderCapabilities, ProviderError, ProviderErrorKind,
    ProviderFuture, ProviderId, SampleKind, Selection, SelectionTarget, Station, StationCandidate,
    TimeRange, WeatherBatch, WeatherProvider, WeatherSample, WeatherSource,
};
use reqwest::{Client, Url};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const API_KEY_URL: &str = "https://opendata.aemet.es/centrodedescargas/altaUsuario";

pub struct Aemet {
    client: Client,
    key: ApiKey,
}

#[derive(Deserialize)]
struct Envelope {
    estado: u16,
    datos: Option<String>,
}

/// Every field is optional, so one incomplete row among the ~900 of `todas` is skipped instead of
/// failing the whole list.
#[derive(Deserialize)]
struct Observation {
    idema: Option<String>,
    ubi: Option<String>,
    lat: Option<f64>,
    lon: Option<f64>,
    alt: Option<f64>,
    fint: Option<String>,
    ta: Option<f64>,
}

impl Aemet {
    pub fn new(client: Client, key: ApiKey) -> Self {
        Self { client, key }
    }

    async fn observations(
        &self,
        station_id: Option<&str>,
    ) -> Result<Vec<Observation>, ProviderError> {
        let mut url =
            Url::parse("https://opendata.aemet.es/opendata/api/observacion/convencional/").unwrap();
        if let Some(id) = station_id {
            url.path_segments_mut()
                .unwrap()
                .pop_if_empty()
                .extend(["datos", "estacion", id]);
        } else {
            url.path_segments_mut()
                .unwrap()
                .pop_if_empty()
                .push("todas");
        }
        let envelope: Envelope = http::json(
            self.client
                .get(url)
                .header("api_key", self.key.expose_secret()),
        )
        .await?;
        if envelope.estado != 200 {
            return Err(http::error(
                match envelope.estado {
                    401 | 403 => ProviderErrorKind::Authentication,
                    429 => ProviderErrorKind::RateLimited,
                    _ => ProviderErrorKind::Unavailable,
                },
                "AEMET no ha podido atender la consulta",
            ));
        }
        let url = envelope
            .datos
            .and_then(|value| Url::parse(&value).ok())
            .filter(|url| {
                url.scheme() == "https"
                    && url.host_str() == Some("opendata.aemet.es")
                    && url.port_or_known_default() == Some(443)
                    && url.username().is_empty()
                    && url.password().is_none()
            })
            .ok_or_else(|| {
                http::error(
                    ProviderErrorKind::InvalidResponse,
                    "AEMET devolvió un enlace de descarga inválido",
                )
            })?;
        http::json(self.client.get(url)).await
    }
}

/// `fint` is documented as UTC without an offset; accept an offset (`Z`, `+02:00`, `+0000`) and
/// fractional seconds too, so a format change does not drop every observation.
fn observation_time(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .or_else(|_| chrono::DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f%z"))
        .map(|dt| dt.timestamp())
        .ok()
        .or_else(|| {
            chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|dt| dt.and_utc().timestamp())
        })
}

fn reading(row: &Observation, now: i64) -> Option<WeatherSample> {
    let ts = observation_time(row.fint.as_deref()?)?;
    let temperature_c = row.ta?;
    (ts >= 0 && ts <= now && temperature_c.is_finite()).then_some(WeatherSample {
        ts,
        temperature_c,
        fetched_at: now,
    })
}

pub fn distance_km(a: Coordinates, b: Coordinates) -> f64 {
    let latitude = ((b.latitude - a.latitude).to_radians() / 2.0).sin().powi(2);
    let longitude = ((b.longitude - a.longitude).to_radians() / 2.0)
        .sin()
        .powi(2);
    let h = latitude + a.latitude.to_radians().cos() * b.latitude.to_radians().cos() * longitude;
    6371.0 * 2.0 * h.clamp(0.0, 1.0).sqrt().asin()
}

impl WeatherProvider for Aemet {
    fn id(&self) -> ProviderId {
        ProviderId::Aemet
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            stations: true,
            requires_api_key: true,
            historical_hourly: false,
        }
    }
    fn temperatures<'a>(
        &'a self,
        selection: &'a Selection,
        range: TimeRange,
    ) -> ProviderFuture<'a, Vec<WeatherBatch>> {
        Box::pin(async move {
            let SelectionTarget::Aemet { station } = &selection.target else {
                return Err(http::error(
                    ProviderErrorKind::Unsupported,
                    "AEMET necesita una estación",
                ));
            };
            let now = crate::app::now();
            let rows = self.observations(Some(&station.id)).await?;
            let samples = rows
                .iter()
                .filter(|row| row.idema.as_deref() == Some(station.id.as_str()))
                .filter_map(|row| reading(row, now))
                .filter(|sample| sample.ts >= range.start && sample.ts < range.end)
                .collect();
            Ok(vec![WeatherBatch {
                source: WeatherSource {
                    provider: ProviderId::Aemet,
                    product: "conventional-observation".into(),
                    kind: SampleKind::Observation,
                    requested_coordinates: station.coordinates,
                    coordinates: station.coordinates,
                    station_id: Some(station.id.clone()),
                    resolution_seconds: 3600,
                    attribution: "AEMET".into(),
                },
                samples,
            }])
        })
    }
    fn stations(&self, location: Coordinates) -> ProviderFuture<'_, Vec<StationCandidate>> {
        Box::pin(async move {
            let now = crate::app::now();
            let mut stations = BTreeMap::<String, StationCandidate>::new();
            for row in self.observations(None).await? {
                let Some(sample) = reading(&row, now).filter(|sample| now - sample.ts <= 7200)
                else {
                    continue;
                };
                let (Some(id), Some(name), Some(latitude), Some(longitude)) =
                    (row.idema, row.ubi, row.lat, row.lon)
                else {
                    continue;
                };
                let station = Station {
                    id,
                    name,
                    coordinates: Coordinates {
                        latitude,
                        longitude,
                    },
                    elevation_m: row.alt,
                };
                if station.validate().is_err() {
                    continue;
                }
                if stations
                    .get(&station.id)
                    .and_then(|s| s.latest_temperature)
                    .is_none_or(|prev| sample.ts > prev.ts)
                {
                    stations.insert(
                        station.id.clone(),
                        StationCandidate {
                            station,
                            latest_temperature: Some(sample),
                        },
                    );
                }
            }
            let mut candidates: Vec<_> = stations.into_values().collect();
            candidates.sort_by(|a, b| {
                distance_km(location, a.station.coordinates)
                    .total_cmp(&distance_km(location, b.station.coordinates))
            });
            Ok(candidates)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn utc_timestamp_is_not_shifted_to_the_previous_hour() {
        assert_eq!(
            observation_time("2026-10-20T10:00:00"),
            observation_time("2026-10-20T10:00:00Z")
        );
        assert_eq!(
            observation_time("2026-10-20T12:00:00+02:00"),
            observation_time("2026-10-20T10:00:00Z")
        );
        assert_eq!(
            observation_time("2026-10-20T10:00:00+0000"),
            observation_time("2026-10-20T10:00:00Z")
        );
        assert_eq!(
            observation_time("2026-10-20T10:00:00.000"),
            observation_time("2026-10-20T10:00:00Z")
        );
        assert_eq!(
            observation_time("2026-10-20T10:00:00.000+0000"),
            observation_time("2026-10-20T10:00:00Z")
        );
        assert!(observation_time("2026-10-20T10:00:00").is_some());
        assert!(observation_time("broken").is_none());
        assert!(
            distance_km(
                Coordinates {
                    latitude: 40.4,
                    longitude: -3.7
                },
                Coordinates {
                    latitude: 40.4,
                    longitude: -3.7
                }
            ) < 0.001
        );
    }

    #[test]
    fn incomplete_rows_are_skipped_not_fatal() {
        let rows: Vec<Observation> = serde_json::from_str(
            r#"[{"idema":"3195","ubi":"MADRID RETIRO","lat":40.41,"lon":-3.68,"fint":"2026-10-20T10:00:00","ta":12.5},
                {"ubi":"SIN ID","lat":40.0,"lon":-3.0,"fint":"2026-10-20T10:00:00","ta":11.0}]"#,
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert!(rows[1].idema.is_none());
    }
}

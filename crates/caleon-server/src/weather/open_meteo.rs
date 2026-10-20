use super::{
    http, Coordinates, Location, ProviderCapabilities, ProviderError, ProviderErrorKind,
    ProviderFuture, ProviderId, SampleKind, Selection, TimeRange, WeatherBatch, WeatherProvider,
    WeatherSample, WeatherSource,
};
use reqwest::Client;
use serde::Deserialize;

pub struct OpenMeteo {
    client: Client,
}

#[derive(Deserialize)]
struct Forecast {
    latitude: f64,
    longitude: f64,
    hourly: Hourly,
}

#[derive(Deserialize)]
struct Hourly {
    time: Vec<i64>,
    temperature_2m: Vec<Option<f64>>,
}

impl OpenMeteo {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub async fn locations(&self, name: &str) -> Result<Vec<Location>, ProviderError> {
        #[derive(Deserialize)]
        struct Places {
            #[serde(default)]
            results: Vec<Place>,
        }
        #[derive(Deserialize)]
        struct Place {
            name: String,
            latitude: f64,
            longitude: f64,
            admin1: Option<String>,
            country: Option<String>,
        }
        if !(2..=120).contains(&name.trim().chars().count()) {
            return Err(http::error(
                ProviderErrorKind::InvalidResponse,
                "Escribe entre 2 y 120 caracteres para buscar una localidad",
            ));
        }
        let data: Places = http::json(
            self.client
                .get("https://geocoding-api.open-meteo.com/v1/search")
                .query(&[("name", name.trim()), ("language", "es"), ("count", "8")]),
        )
        .await?;
        Ok(data
            .results
            .into_iter()
            .filter_map(|place| {
                let coordinates = Coordinates {
                    latitude: place.latitude,
                    longitude: place.longitude,
                };
                coordinates.validate().ok()?;
                let mut parts = vec![place.name];
                parts.extend(place.admin1);
                parts.extend(place.country);
                parts.dedup();
                Some(Location {
                    name: parts.join(", "),
                    coordinates,
                })
            })
            .collect())
    }
}

fn normalize(
    data: Forecast,
    selection: &Selection,
    range: TimeRange,
    fetched_at: i64,
) -> Result<WeatherBatch, ProviderError> {
    if data.hourly.time.len() != data.hourly.temperature_2m.len() {
        return Err(http::error(
            ProviderErrorKind::InvalidResponse,
            "Open-Meteo devolvió una serie incompleta",
        ));
    }
    let source = WeatherSource {
        provider: ProviderId::OpenMeteo,
        product: "forecast-best-match".into(),
        kind: SampleKind::Estimate,
        requested_coordinates: selection.location.coordinates,
        coordinates: Coordinates {
            latitude: data.latitude,
            longitude: data.longitude,
        },
        station_id: None,
        resolution_seconds: 3600,
        attribution: "Open-Meteo · CC BY 4.0".into(),
    };
    source.validate().map_err(|_| {
        http::error(
            ProviderErrorKind::InvalidResponse,
            "Open-Meteo devolvió coordenadas inválidas",
        )
    })?;
    let samples = data
        .hourly
        .time
        .into_iter()
        .zip(data.hourly.temperature_2m)
        .filter_map(|(ts, temperature)| {
            let temperature_c = temperature?;
            (ts >= range.start && ts < range.end && ts <= fetched_at && temperature_c.is_finite())
                .then_some(WeatherSample {
                    ts,
                    temperature_c,
                    fetched_at,
                })
        })
        .collect();
    Ok(WeatherBatch { source, samples })
}

impl WeatherProvider for OpenMeteo {
    fn id(&self) -> ProviderId {
        ProviderId::OpenMeteo
    }
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            stations: false,
            requires_api_key: false,
            historical_hourly: true,
        }
    }
    fn temperatures<'a>(
        &'a self,
        selection: &'a Selection,
        range: TimeRange,
    ) -> ProviderFuture<'a, Vec<WeatherBatch>> {
        Box::pin(async move {
            range
                .validate()
                .and_then(|_| selection.validate())
                .map_err(|_| {
                    http::error(
                        ProviderErrorKind::InvalidResponse,
                        "La ubicación o el periodo no son válidos",
                    )
                })?;
            let now = crate::app::now();
            let past_days = ((now - range.start).max(0) / 86400 + 1).min(92);
            let data = http::json(
                self.client
                    .get("https://api.open-meteo.com/v1/forecast")
                    .query(&[
                        (
                            "latitude",
                            selection.location.coordinates.latitude.to_string(),
                        ),
                        (
                            "longitude",
                            selection.location.coordinates.longitude.to_string(),
                        ),
                        ("hourly", "temperature_2m".into()),
                        ("temperature_unit", "celsius".into()),
                        ("timeformat", "unixtime".into()),
                        ("timezone", "UTC".into()),
                        ("past_days", past_days.to_string()),
                        ("forecast_days", "1".into()),
                    ]),
            )
            .await?;
            Ok(vec![normalize(data, selection, range, now)?])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hourly_data_keeps_null_gaps_and_excludes_future() {
        let data: Forecast = serde_json::from_value(serde_json::json!({"latitude":40.4,"longitude":-3.7,"hourly":{"time":[0,3600,7200,10800],"temperature_2m":[5,null,7,8]}})).unwrap();
        let selection = Selection {
            location: Location {
                name: "Casa".into(),
                coordinates: Coordinates {
                    latitude: 40.41,
                    longitude: -3.71,
                },
            },
            target: super::super::SelectionTarget::OpenMeteo,
        };
        let batch = normalize(
            data,
            &selection,
            TimeRange {
                start: 0,
                end: 12000,
            },
            8000,
        )
        .unwrap();
        assert_eq!(
            batch.samples.iter().map(|s| s.ts).collect::<Vec<_>>(),
            vec![0, 7200]
        );
        assert_eq!(
            batch.source.requested_coordinates,
            selection.location.coordinates
        );
        assert_ne!(batch.source.coordinates, selection.location.coordinates);
    }
}

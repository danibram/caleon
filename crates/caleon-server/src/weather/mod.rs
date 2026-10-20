//! Weather selections and normalized temperatures retain their provider and location.

pub mod aemet;
mod config;
mod http;
pub mod open_meteo;
mod provider;
pub mod runtime;

pub use config::{ApiKey, WeatherConfig, WeatherSettings};
pub use provider::{
    ProviderCapabilities, ProviderError, ProviderErrorKind, ProviderFuture, WeatherProvider,
};

use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderId {
    OpenMeteo,
    Aemet,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
}

impl Coordinates {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.latitude.is_finite() && (-90.0..=90.0).contains(&self.latitude),
            "invalid latitude"
        );
        ensure!(
            self.longitude.is_finite() && (-180.0..=180.0).contains(&self.longitude),
            "invalid longitude"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Location {
    pub name: String,
    pub coordinates: Coordinates,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub coordinates: Coordinates,
    pub elevation_m: Option<f64>,
}

impl Station {
    pub fn validate(&self) -> Result<()> {
        ensure!(!self.id.trim().is_empty(), "station id is required");
        ensure!(!self.name.trim().is_empty(), "station name is required");
        self.coordinates.validate()?;
        ensure!(
            self.elevation_m.is_none_or(f64::is_finite),
            "invalid station elevation"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "provider", rename_all = "kebab-case")]
pub enum SelectionTarget {
    OpenMeteo,
    Aemet { station: Station },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Selection {
    pub location: Location,
    pub target: SelectionTarget,
}

impl Selection {
    pub fn provider(&self) -> ProviderId {
        match self.target {
            SelectionTarget::OpenMeteo => ProviderId::OpenMeteo,
            SelectionTarget::Aemet { .. } => ProviderId::Aemet,
        }
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.location.name.trim().is_empty(),
            "location name is required"
        );
        self.location.coordinates.validate()?;
        if let SelectionTarget::Aemet { station } = &self.target {
            station.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum SampleKind {
    Observation,
    Estimate,
    Reanalysis,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct WeatherSource {
    pub provider: ProviderId,
    pub product: String,
    pub kind: SampleKind,
    pub requested_coordinates: Coordinates,
    pub coordinates: Coordinates,
    pub station_id: Option<String>,
    pub resolution_seconds: u32,
    pub attribution: String,
}

impl WeatherSource {
    pub fn validate(&self) -> Result<()> {
        self.requested_coordinates.validate()?;
        self.coordinates.validate()?;
        ensure!(
            !self.product.trim().is_empty(),
            "weather product is required"
        );
        ensure!(
            !self.attribution.trim().is_empty(),
            "weather attribution is required"
        );
        ensure!(
            self.resolution_seconds > 0,
            "weather resolution must be positive"
        );
        match self.provider {
            ProviderId::Aemet => {
                ensure!(
                    self.station_id
                        .as_ref()
                        .is_some_and(|id| !id.trim().is_empty()),
                    "AEMET source requires a station"
                );
                ensure!(
                    self.kind == SampleKind::Observation,
                    "AEMET source must contain observations"
                );
            }
            ProviderId::OpenMeteo => {
                ensure!(
                    self.station_id.is_none(),
                    "Open-Meteo sources use coordinates"
                );
                ensure!(
                    self.kind != SampleKind::Observation,
                    "Open-Meteo source must contain model data"
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct WeatherSample {
    #[ts(type = "number")]
    pub ts: i64,
    pub temperature_c: f64,
    #[ts(type = "number")]
    pub fetched_at: i64,
}

impl WeatherSample {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.temperature_c.is_finite(),
            "weather temperature must be finite"
        );
        ensure!(
            self.ts >= 0 && self.fetched_at >= self.ts,
            "invalid weather observation time"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TimeRange {
    #[ts(type = "number")]
    pub start: i64,
    #[ts(type = "number")]
    pub end: i64,
}

impl TimeRange {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.start >= 0 && self.start < self.end,
            "invalid weather time range"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SelectionPeriod {
    #[ts(type = "number")]
    pub id: i64,
    pub selection: Selection,
    pub range: TimeRange,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct WeatherPoint {
    #[ts(type = "number")]
    pub period_id: i64,
    #[ts(type = "number")]
    pub source_id: i64,
    pub source: WeatherSource,
    pub sample: WeatherSample,
}

#[derive(Debug, Clone)]
pub struct WeatherBatch {
    pub source: WeatherSource,
    pub samples: Vec<WeatherSample>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct StationCandidate {
    pub station: Station,
    pub latest_temperature: Option<WeatherSample>,
}

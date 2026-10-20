use super::{Coordinates, ProviderId, Selection, StationCandidate, TimeRange, WeatherBatch};
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt, future::Future, pin::Pin};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
pub struct ProviderCapabilities {
    pub stations: bool,
    pub requires_api_key: bool,
    pub historical_hourly: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorKind {
    Authentication,
    Unavailable,
    RateLimited,
    InvalidResponse,
    Unsupported,
}

#[derive(Debug)]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub message: String,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for ProviderError {}

pub type ProviderFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, ProviderError>> + Send + 'a>>;

pub trait WeatherProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn capabilities(&self) -> ProviderCapabilities;

    /// Returns past temperatures in the half-open UTC range [start, end).
    fn temperatures<'a>(
        &'a self,
        selection: &'a Selection,
        range: TimeRange,
    ) -> ProviderFuture<'a, Vec<WeatherBatch>>;

    fn stations(&self, _location: Coordinates) -> ProviderFuture<'_, Vec<StationCandidate>> {
        Box::pin(async {
            Err(ProviderError {
                kind: ProviderErrorKind::Unsupported,
                message: "provider does not offer weather stations".into(),
            })
        })
    }
}

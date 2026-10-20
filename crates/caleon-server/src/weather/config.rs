use super::{Selection, SelectionTarget};
use crate::credentials::{self, Credentials};
use crate::store::Store;
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::Path;
use ts_rs::TS;

#[derive(Clone, PartialEq)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(value: &str) -> Result<Self> {
        ensure!(!value.trim().is_empty(), "AEMET API key is required");
        Ok(Self(value.trim().to_owned()))
    }

    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WeatherConfig {
    pub enabled: bool,
    pub selection: Option<Selection>,
    pub aemet_api_key: Option<ApiKey>,
    pub selection_since: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WeatherSettings {
    pub enabled: bool,
    pub selection: Option<Selection>,
    pub aemet_key_configured: bool,
}

impl WeatherConfig {
    /// Structural validation does not verify a credential against its provider.
    pub fn validate(&self) -> Result<()> {
        if let Some(selection) = &self.selection {
            selection.validate()?;
        }
        if self.enabled {
            let selection = self
                .selection
                .as_ref()
                .context("weather selection is required")?;
            if matches!(selection.target, SelectionTarget::Aemet { .. }) {
                ensure!(
                    self.aemet_api_key
                        .as_ref()
                        .is_some_and(|key| !key.0.trim().is_empty()),
                    "AEMET API key is required"
                );
            }
        }
        Ok(())
    }

    pub fn settings(&self) -> WeatherSettings {
        WeatherSettings {
            enabled: self.enabled,
            selection: self.selection.clone(),
            aemet_key_configured: self
                .aemet_api_key
                .as_ref()
                .is_some_and(|key| !key.0.trim().is_empty()),
        }
    }

    pub fn can_collect(&self, history_enabled: bool) -> bool {
        self.enabled && history_enabled && self.validate().is_ok()
    }

    fn database(data_dir: &Path) -> Result<Connection> {
        fs::create_dir_all(data_dir)?;
        let db = Connection::open(data_dir.join("history.sqlite"))?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS weather_config (
               id INTEGER PRIMARY KEY CHECK(id=1), enabled INTEGER NOT NULL,
               selection TEXT, selection_since INTEGER);",
        )?;
        credentials::initialize(&db)?;
        Ok(db)
    }

    fn read(db: &Connection, data_dir: &Path) -> Result<Option<Self>> {
        let row = db
            .query_row(
                "SELECT enabled, selection, selection_since FROM weather_config WHERE id=1",
                [],
                |r| {
                    Ok((
                        r.get::<_, bool>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((enabled, selection, selection_since)) = row else {
            return Ok(None);
        };
        let config = Self {
            enabled,
            selection: selection
                .map(|value| serde_json::from_str(&value))
                .transpose()
                .context("invalid weather selection")?,
            selection_since,
            aemet_api_key: Credentials::new(db, data_dir)
                .get("weather.aemet.api_key")?
                .map(|value| ApiKey::new(&value))
                .transpose()?,
        };
        config.validate()?;
        Ok(Some(config))
    }

    fn write(&self, db: &Connection, data_dir: &Path) -> Result<()> {
        let credentials = Credentials::new(db, data_dir);
        if let Some(key) = &self.aemet_api_key {
            credentials.set("weather.aemet.api_key", key.expose_secret())?;
        } else {
            db.execute(
                "DELETE FROM credentials WHERE name=?1",
                ["weather.aemet.api_key"],
            )?;
        }
        db.execute(
            "INSERT INTO weather_config (id, enabled, selection, selection_since) VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET enabled=excluded.enabled,
               selection=excluded.selection, selection_since=excluded.selection_since",
            params![self.enabled, self.selection.as_ref().map(serde_json::to_string).transpose()?, self.selection_since],
        )?;
        Ok(())
    }

    pub fn load(data_dir: &Path) -> Result<Self> {
        let mut db = Self::database(data_dir)?;
        let tx = db.transaction()?;
        let config = Self::read(&tx, data_dir)?.unwrap_or_default();
        tx.commit()?;
        Ok(config)
    }

    pub fn save(&self, data_dir: &Path) -> Result<()> {
        let mut config = self.clone();
        if config.enabled {
            config
                .selection_since
                .get_or_insert(crate::app::now() / 3600 * 3600);
        }
        config.validate()?;
        let mut db = Self::database(data_dir)?;
        let tx = db.transaction_with_behavior(TransactionBehavior::Immediate)?;
        Self::read(&tx, data_dir)?;
        config.write(&tx, data_dir)?;
        if config.enabled {
            Store::select_weather_source_in(
                &tx,
                config.selection.as_ref().unwrap(),
                config.selection_since.unwrap(),
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

use super::Store;
use crate::weather::{
    ProviderId, SampleKind, Selection, SelectionPeriod, SelectionTarget, TimeRange, WeatherPoint,
    WeatherSample, WeatherSource,
};
use anyhow::{ensure, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

pub(super) fn initialize(db: &Connection) -> Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS weather_selections (
           id INTEGER PRIMARY KEY, started_at INTEGER NOT NULL CHECK(started_at >= 0),
           ended_at INTEGER CHECK(ended_at >= started_at), selection TEXT NOT NULL);
         CREATE UNIQUE INDEX IF NOT EXISTS weather_selection_active
           ON weather_selections((1)) WHERE ended_at IS NULL;
         CREATE INDEX IF NOT EXISTS weather_selection_time ON weather_selections(started_at, ended_at);
         CREATE TABLE IF NOT EXISTS weather_sources (
           id INTEGER PRIMARY KEY, source TEXT NOT NULL UNIQUE, priority INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS weather_samples (
           selection_id INTEGER NOT NULL REFERENCES weather_selections(id),
           source_id INTEGER NOT NULL REFERENCES weather_sources(id),
           ts INTEGER NOT NULL CHECK(ts >= 0), temperature REAL NOT NULL,
           fetched_at INTEGER NOT NULL CHECK(fetched_at >= ts),
           PRIMARY KEY(selection_id, source_id, ts));
         CREATE INDEX IF NOT EXISTS weather_samples_time ON weather_samples(ts);",
    )?;
    Ok(())
}

impl Store {
    /// The first selection can start at the beginning of an eligible historical import.
    pub fn select_weather_source(&self, selection: &Selection, effective_from: i64) -> Result<i64> {
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        let id = Self::select_weather_source_in(&tx, selection, effective_from)?;
        tx.commit()?;
        Ok(id)
    }

    pub(crate) fn select_weather_source_in(
        db: &Connection,
        selection: &Selection,
        effective_from: i64,
    ) -> Result<i64> {
        initialize(db)?;
        selection.validate()?;
        ensure!(effective_from >= 0, "invalid weather selection time");
        let encoded = serde_json::to_string(selection)?;
        let current: Option<(i64, i64, String)> = db
            .query_row(
                "SELECT id, started_at, selection FROM weather_selections WHERE ended_at IS NULL",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        if let Some((id, started_at, previous)) = current {
            ensure!(
                effective_from >= started_at,
                "weather selection cannot move backwards"
            );
            if previous == encoded {
                return Ok(id);
            }
            ensure!(
                !db.query_row(
                    "SELECT EXISTS(SELECT 1 FROM weather_samples WHERE selection_id = ?1 AND ts >= ?2)",
                    params![id, effective_from], |r| r.get::<_, bool>(0),
                )?,
                "weather selection change would reassign existing samples"
            );
            db.execute(
                "UPDATE weather_selections SET ended_at = ?1 WHERE id = ?2",
                params![effective_from, id],
            )?;
        }
        db.execute(
            "INSERT INTO weather_selections(started_at, selection) VALUES (?1, ?2)",
            params![effective_from, encoded],
        )?;
        let id = db.last_insert_rowid();
        Ok(id)
    }

    pub fn weather_selections(&self, range: TimeRange) -> Result<Vec<SelectionPeriod>> {
        range.validate()?;
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare(
            "SELECT id, selection, MAX(started_at, ?1), MIN(COALESCE(ended_at, ?2), ?2)
             FROM weather_selections WHERE started_at < ?2 AND (ended_at IS NULL OR ended_at > ?1)
             AND (ended_at IS NULL OR ended_at > started_at) ORDER BY started_at, id",
        )?;
        let rows = stmt.query_map(params![range.start, range.end], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (id, selection, start, end) = row?;
            Ok(SelectionPeriod {
                id,
                selection: serde_json::from_str(&selection)?,
                range: TimeRange { start, end },
            })
        })
        .collect()
    }

    pub fn add_weather_samples(
        &self,
        period_id: i64,
        source: &WeatherSource,
        samples: &[WeatherSample],
    ) -> Result<()> {
        source.validate()?;
        for sample in samples {
            sample.validate()?;
        }
        let mut db = self.db.lock().unwrap();
        let tx = db.transaction()?;
        let (start, end, encoded): (i64, Option<i64>, String) = tx
            .query_row(
                "SELECT started_at, ended_at, selection FROM weather_selections WHERE id = ?1",
                [period_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .context("weather selection does not exist")?;
        let selection: Selection = serde_json::from_str(&encoded)?;
        let requested = match (&selection.target, source.provider) {
            (SelectionTarget::Aemet { station }, ProviderId::Aemet) => {
                ensure!(
                    source.station_id.as_deref() == Some(station.id.as_str()),
                    "weather station does not match selection"
                );
                station.coordinates
            }
            (_, ProviderId::OpenMeteo) => selection.location.coordinates,
            _ => anyhow::bail!("weather provider does not match selection"),
        };
        ensure!(
            source.requested_coordinates == requested,
            "weather location does not match selection"
        );
        for sample in samples {
            ensure!(
                sample.ts >= start && end.is_none_or(|end| sample.ts < end),
                "weather sample falls outside its selection period"
            );
        }
        if samples.is_empty() {
            return Ok(());
        }
        let encoded_source = serde_json::to_string(source)?;
        let priority = if source.kind == SampleKind::Observation {
            0
        } else {
            1
        };
        tx.execute("INSERT INTO weather_sources(source, priority) VALUES (?1, ?2) ON CONFLICT(source) DO NOTHING", params![encoded_source, priority])?;
        let source_id: i64 = tx.query_row(
            "SELECT id FROM weather_sources WHERE source = ?1",
            [&encoded_source],
            |r| r.get(0),
        )?;
        for sample in samples {
            tx.execute(
                "INSERT INTO weather_samples(selection_id, source_id, ts, temperature, fetched_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(selection_id, source_id, ts) DO UPDATE
                 SET temperature = excluded.temperature, fetched_at = excluded.fetched_at
                 WHERE excluded.fetched_at >= weather_samples.fetched_at",
                params![
                    period_id,
                    source_id,
                    sample.ts,
                    sample.temperature_c,
                    sample.fetched_at
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Returns every origin, or the preferred origin at each timestamp when requested.
    pub fn weather_history(
        &self,
        range: TimeRange,
        preferred_only: bool,
    ) -> Result<Vec<WeatherPoint>> {
        range.validate()?;
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare(
            "SELECT selection_id, source_id, source, ts, temperature, fetched_at FROM (
               SELECT w.*, s.source,
                 ROW_NUMBER() OVER (PARTITION BY selection_id, ts ORDER BY s.priority, fetched_at DESC, source_id DESC) AS rank
               FROM weather_samples w JOIN weather_sources s ON s.id = w.source_id
               WHERE ts >= ?1 AND ts < ?2
             ) WHERE ?3 = 0 OR rank = 1 ORDER BY ts, selection_id, source_id",
        )?;
        let rows = stmt.query_map(params![range.start, range.end, preferred_only], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        })?;
        rows.map(|row| {
            let (period_id, source_id, source, ts, temperature_c, fetched_at) = row?;
            Ok(WeatherPoint {
                period_id,
                source_id,
                source: serde_json::from_str(&source)?,
                sample: WeatherSample {
                    ts,
                    temperature_c,
                    fetched_at,
                },
            })
        })
        .collect()
    }

    pub fn indoor_temperature_ranges(
        &self,
        range: TimeRange,
        bucket_seconds: u32,
    ) -> Result<Vec<TimeRange>> {
        range.validate()?;
        ensure!(bucket_seconds > 0, "indoor bucket must be positive");
        let db = self.db.lock().unwrap();
        let mut stmt = db.prepare(
            "SELECT DISTINCT (ts / ?3) * ?3 FROM samples
             WHERE ts >= ?1 AND ts < ?2 AND temperature IS NOT NULL ORDER BY 1",
        )?;
        let rows = stmt.query_map(params![range.start, range.end, bucket_seconds], |r| {
            r.get::<_, i64>(0)
        })?;
        let mut ranges: Vec<TimeRange> = Vec::new();
        for row in rows {
            let bucket = row?;
            let start = bucket.max(range.start);
            let end = bucket
                .saturating_add(i64::from(bucket_seconds))
                .min(range.end);
            if let Some(last) = ranges.last_mut() {
                if last.end == start {
                    last.end = end;
                    continue;
                }
            }
            ranges.push(TimeRange { start, end });
        }
        Ok(ranges)
    }
}

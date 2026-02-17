mod models;

use std::time::Duration;
use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use reqwest::blocking::Client;
use anyhow::Result;
use thiserror::Error;
use crate::config::Config;
use crate::models::{ForecastValue, ForecastValues};
use crate::manager_forecast::models::ForecastRecord;

/// Struct for managing whether forecasts
pub struct Forecast {
    client: Client,
    host: String,
    port: u16,
    high_clouds_factor: f64,
    mid_clouds_factor: f64,
    low_clouds_factor: f64,
}

impl Forecast {
    /// Returns a forecast struct ready for fetching and processing whether forecasts
    ///
    /// # Arguments
    ///
    /// * 'config' - configuration to use
    pub fn new(config: &Config) -> Result<Forecast, ForecastError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Forecast {
            client,
            host: config.forecast.host.clone(),
            port: config.forecast.port,           
            high_clouds_factor: config.production.high_clouds_factor,
            mid_clouds_factor: config.production.mid_clouds_factor,
            low_clouds_factor: config.production.low_clouds_factor, 
        })
    }

    /// Retrieves a weather forecast for the given date
    ///
    /// # Arguments
    ///
    /// * 'from' - the datetime to get forecast from
    /// * 'to' - the datetime to get forecast to (non-inclusive)
    pub fn new_forecast(&self, from: DateTime<Utc>, to: DateTime<Utc>) -> Result<ForecastValues, ForecastError> {
        let from = from.duration_trunc(TimeDelta::hours(1))
            .map_err(|e| ForecastError::DateError(format!("from date: {}", e.to_string())))?;
        let to = to.duration_trunc(TimeDelta::hours(1))
            .map_err(|e| ForecastError::DateError(format!("to date: {}", e.to_string())))?;

        let url = format!("http://{}:{}/forecast", self.host, self.port);

        let response = self.client
            .get(url)
            .query(&vec![("id", "smhi"), ("from", &from.to_rfc3339()), ("to", &to.to_rfc3339())])
            .send()?;
        
        let json = response.text()?;

        let tmp_forecast: Vec<ForecastRecord> = serde_json::from_str(&json)
            .map_err(|e| ForecastError::ParseError(e.to_string()))?;

        let mut forecast: Vec<ForecastValue> = Vec::new();

        for fr in tmp_forecast {
            let (lcc_mean, mcc_mean, hcc_mean, cloud_factor) = self.cloud_factor(fr.lcc_mean, fr.mcc_mean, fr.hcc_mean);
            let fc = ForecastValue {
                valid_time: fr.date_time,
                temp: fr.temperature,
                lcc_mean,
                mcc_mean,
                hcc_mean,
                cloud_factor,
            };

            forecast.push(fc);
        }


        if forecast.len() == 0 {
            Err(ForecastError::EmptyForecastError(format!("No forecast found for {} - {}", from, to)))
        } else {
            Ok(ForecastValues{forecast})
        }
    }

    /// Calculates the cloud factor
    ///
    /// # Arguments
    ///
    /// * 'lcc_mean' - low height cloud factor from forecast (0-8)
    /// * 'mcc_mean' - medium height cloud factor from forecast (0-8)
    /// * 'hcc_mean' - high height cloud factor from forecast (0-8) 
    fn cloud_factor(&self, lcc_mean: u8, mcc_mean: u8, hcc_mean: u8) -> (f64, f64, f64, f64) {
        let lcc_mean = lcc_mean as f64;
        let mcc_mean = mcc_mean as f64;
        let hcc_mean = hcc_mean as f64;
        
        let cf = (1.0 - hcc_mean / 8.0 * self.high_clouds_factor) *
            (1.0 - mcc_mean / 8.0 * self.mid_clouds_factor) *
            (1.0 - lcc_mean / 8.0 * self.low_clouds_factor);

        (lcc_mean, mcc_mean, hcc_mean, cf)
    }
}

#[derive(Error, Debug)]
pub enum ForecastError {
    #[error("DateError: {0}")]
    DateError(String),
    #[error("ParseError: {0}")]
    ParseError(String),
    #[error("EmptyForecastError: {0}")]
    EmptyForecastError(String),
    #[error("NetworkError: {0}")]
    NetworkError(#[from] reqwest::Error),
}
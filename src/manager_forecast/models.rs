use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ForecastRecord {
    pub date_time: DateTime<Utc>,
    pub temperature: f64,
    pub lcc_mean: u8,
    pub mcc_mean: u8,
    pub hcc_mean: u8,
}

use chrono::{DateTime, Local};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct ProductionValues {
    pub valid_time: DateTime<Local>,
    pub power: f64
}

#[derive(Deserialize, Debug)]
pub struct ConsumptionValues {
    pub valid_time: DateTime<Local>,
    pub power: f64
}

#[derive(Deserialize, Debug)]
pub struct TariffValues {
    pub valid_time: DateTime<Local>,
    pub price: f64,
    pub buy: f64,
    pub sell: f64,
}

#[derive(Deserialize, Debug)]
pub struct BackupData {
    pub date_time: DateTime<Local>,
    pub production: Vec<ProductionValues>,
    pub consumption: Vec<ConsumptionValues>,
    pub tariffs: Vec<TariffValues>,
}
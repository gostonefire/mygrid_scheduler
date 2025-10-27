use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug)]
pub struct TariffValue {
    pub valid_time: DateTime<Local>,
    pub price: f64,
    pub buy: f64,
    pub sell: f64,
}

#[derive(Clone, Serialize, Debug)]
pub struct PowerValue {
    pub valid_time: DateTime<Local>,
    pub power: f64
}

#[derive(Debug)]
pub struct PowerValues {
    pub power: Vec<PowerValue>,
}

#[derive(Debug)]
pub struct BackupData {
    pub date_time: DateTime<Local>,
    pub production: Vec<PowerValue>,
    pub consumption: Vec<PowerValue>,
    pub tariffs: Vec<TariffValue>,
}


#[derive(Deserialize, Debug)]
pub struct EntryPerArea {
    #[serde(rename = "SE4")]
    pub se4: f64,
}

#[derive(Deserialize, Debug)]
pub struct MultiAreaEntries {
    #[serde(rename = "deliveryStart")]
    pub delivery_start: DateTime<Local>,
    #[serde(rename = "entryPerArea")]
    pub entry_per_area: EntryPerArea,
}

#[derive(Deserialize, Debug)]
pub struct Tariffs {
    #[serde(rename = "multiAreaEntries")]
    pub multi_area_entries: Vec<MultiAreaEntries>,
}

#[derive(Deserialize)]
pub struct ForecastRecord {
    pub date_time: DateTime<Local>,
    pub temperature: f64,
    pub lcc_mean: u8,
    pub mcc_mean: u8,
    pub hcc_mean: u8,
}

#[derive(Serialize, Debug)]
pub struct ForecastValue {
    pub valid_time: DateTime<Local>,
    pub temp: f64,
    pub lcc_mean: f64,
    pub mcc_mean: f64,
    pub hcc_mean: f64,
    pub cloud_factor: f64,
}

#[derive(Debug)]
pub struct ForecastValues {
    pub forecast: Vec<ForecastValue>,
}

#[derive(Serialize)]
pub struct RequestCurrentSoc {
    pub sn: String,
    pub variables: Vec<String>,
}

#[derive(Deserialize)]
pub struct SocCurrentData {
    pub value: f64,
}

#[derive(Deserialize)]
pub struct SocCurrentVariables {
    pub datas: Vec<SocCurrentData>,
}

#[derive(Deserialize)]
pub struct SocCurrentResult {
    pub result: Vec<SocCurrentVariables>,
}

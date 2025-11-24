use chrono::{DateTime, Utc};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct EntryPerArea {
    #[serde(rename = "SE4")]
    pub se4: f64,
}

#[derive(Deserialize, Debug)]
pub struct MultiAreaEntries {
    #[serde(rename = "deliveryStart")]
    pub delivery_start: DateTime<Utc>,
    #[serde(rename = "entryPerArea")]
    pub entry_per_area: EntryPerArea,
}

#[derive(Deserialize, Debug)]
pub struct Tariffs {
    #[serde(rename = "multiAreaEntries")]
    pub multi_area_entries: Vec<MultiAreaEntries>,
}

pub mod errors;
mod models;

use std::time::Duration;
use chrono::{DateTime, Local};
use ureq::Agent;
use anyhow::Result;
use crate::manager_nordpool::errors::NordPoolError;
use crate::common::models::{TariffValue};
use crate::manager_nordpool::models::Tariffs;

pub struct NordPool {
    agent: Agent,
}

impl NordPool {
    pub fn new() -> NordPool {
        let config = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build();

        let agent = config.into();

        Self { agent }
    }

    /// Retrieves day ahead prices from NordPool
    /// It gets the tariffs for the day indicated by date_time (if it can't an error will be returned),
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the date to retrieve prices for
    pub fn get_tariffs(&self, date_time: DateTime<Local>) -> Result<Vec<TariffValue>> {
        let result = self.get_day_tariffs(date_time)?;

        Ok(result)
    }

    /// Retrieves day ahead prices from NordPool
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the date to retrieve prices for
    fn get_day_tariffs(&self, date_time: DateTime<Local>) -> Result<Vec<TariffValue>> {
        // https://dataportal-api.nordpoolgroup.com/api/DayAheadPrices?date=2025-10-22&market=DayAhead&deliveryArea=SE4&currency=SEK
        let url = "https://dataportal-api.nordpoolgroup.com/api/DayAheadPrices";
        let date = format!("{}", date_time.format("%Y-%m-%d"));
        let query = vec![
            ("date", date.as_str()),
            ("market", "DayAhead"),
            ("deliveryArea", "SE4"),
            ("currency", "SEK"),
        ];

        let mut response = self.agent
            .get(url)
            .query_pairs(query)
            .call()?;

        if response.status() == 204 {
            return Err(NordPoolError::NoContent)?;
        }

        let json = response
            .body_mut()
            .read_to_string()?;

        let tariffs: Tariffs = serde_json::from_str(&json)?;
        NordPool::tariffs_to_vec(&tariffs)
    }

    /// Transforms the Tariffs struct to a plain vector of prices
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - the struct containing prices
    fn tariffs_to_vec(tariffs: &Tariffs) -> Result<Vec<TariffValue>> {
        if tariffs.multi_area_entries.len() != 96 {
            return Err(NordPoolError::Document("number of day tariffs not equal to 96".into()))?
        }

        let mut result: Vec<TariffValue> = Vec::new();
        tariffs.multi_area_entries.iter().for_each(
            |t| {
                result.push(add_vat_markup(t.entry_per_area.se4, t.delivery_start));
            });

        Ok(result)
    }
}

/// Adds VAT and other markups such as energy taxes etc.
///
/// The function spits out one buy price and one sell price
/// Buy:
/// * - Net fee: 31.625 öre (inc VAT)
/// * - Spot fee: 7.7% (excl VAT)
/// * - Energy taxes: 54.875 öre (inc VAT)
/// * - Spot price (excl VAT)
/// * - Variable fees: 7.696 öre (excl VAT)
/// * - Extra: 2.4 öre (excl VAT)
///
/// Sell:
/// * - Spot price (no VAT)
/// * - Extra: 7.5 öre (no VAT)
/// 
/// Sell, but not included in calculation to only focus on day-by-day
/// * - Tax reduction: 60 öre (no VAT), is returned yearly together with tax regulation
///
/// # Arguments
///
/// * 'tariff' - spot fee as from NordPool in SEK/MWh
/// * 'delivery_start' - start time for the spot
fn add_vat_markup(tariff: f64, delivery_start: DateTime<Local>) -> TariffValue {
    let price = tariff / 1000.0; // SEK per MWh to per kWh
    let buy = 0.31625 + (0.077 * price) / 0.8 + 0.54875 + (price + 0.024 + 0.07696) / 0.8;
    let sell = 0.075 + price;

    TariffValue {
        valid_time: delivery_start,
        price: round_to_two_decimals(price),
        buy: round_to_two_decimals(buy),
        sell: round_to_two_decimals(sell),
    }
}

/// Rounds values to two decimals
///
/// # Arguments
///
/// * 'price' - the price to round to two decimals
fn round_to_two_decimals(price: f64) -> f64 {
    (price * 100f64).round() / 100f64
}
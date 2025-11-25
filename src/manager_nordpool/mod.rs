pub mod errors;
mod models;

use std::ops::Add;
use std::time::Duration;
use chrono::{DateTime, TimeDelta, Utc};
use ureq::Agent;
use anyhow::Result;
use crate::manager_nordpool::errors::NordPoolError;
use crate::common::models::{TariffValue};
use crate::config::TariffFees;
use crate::manager_nordpool::models::Tariffs;

pub struct NordPool {
    agent: Agent,
    variable_fee: f64,
    spot_fee_percentage: f64,
    energy_tax: f64,
    swedish_power_grid: f64,
    balance_responsibility: f64,
    electric_certificate: f64,
    guarantees_of_origin: f64,
    fixed: f64,
    production_price: f64,
}

impl NordPool {
    pub fn new(config: &TariffFees) -> NordPool {
        let agent_config = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build();

        let agent = agent_config.into();

        Self {
            agent,
            variable_fee: config.variable_fee,
            spot_fee_percentage: config.spot_fee_percentage / 100.0,
            energy_tax: config.energy_tax,
            swedish_power_grid: config.swedish_power_grid,
            balance_responsibility: config.balance_responsibility,
            electric_certificate: config.electric_certificate,
            guarantees_of_origin: config.guarantees_of_origin,
            fixed: config.fixed,
            production_price: config.production_price,
        }
    }

    /// Retrieves day ahead prices from NordPool
    /// It gets the tariffs for the day indicated by date_time (if it can't an error will be returned),
    ///
    /// # Arguments
    ///
    /// * 'day_start' - the start time of the day to retrieve prices for
    /// * 'day_date' - the date to retrieve prices for
    pub fn get_tariffs(&self, day_start: DateTime<Utc>, day_date: DateTime<Utc>) -> Result<Vec<TariffValue>> {
        let result = self.get_day_tariffs(day_start, day_date)?;

        Ok(result)
    }

    /// Retrieves day ahead prices from NordPool
    ///
    /// # Arguments
    ///
    /// * 'day_start' - the start time of the day to retrieve prices for
    /// * 'day_date' - the date to retrieve prices for
    fn get_day_tariffs(&self, day_start: DateTime<Utc>, day_date: DateTime<Utc>) -> Result<Vec<TariffValue>> {
        // https://dataportal-api.nordpoolgroup.com/api/DayAheadPrices?date=2025-10-22&market=DayAhead&deliveryArea=SE4&currency=SEK
        let url = "https://dataportal-api.nordpoolgroup.com/api/DayAheadPrices";
        let date = format!("{}", day_date.format("%Y-%m-%d"));
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
        self.tariffs_to_vec(&tariffs, day_start)
    }

    /// Transforms the Tariffs struct to a plain vector of prices
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - the struct containing prices
    fn tariffs_to_vec(&self, tariffs: &Tariffs, day_start: DateTime<Utc>) -> Result<Vec<TariffValue>> {
        let entries = tariffs.multi_area_entries.len();
        if entries < 96 {
            return Err(NordPoolError::Document("number of day tariffs less than 96".into()))?
        }
        let day_end = day_start.add(TimeDelta::hours(24));
        let day_avg = tariffs.multi_area_entries.iter().map(|t| t.entry_per_area.se4).sum::<f64>() / entries as f64 / 1000.0;

        let mut result: Vec<TariffValue> = Vec::new();
        tariffs.multi_area_entries.iter().filter(|t| t.delivery_start >= day_start && t.delivery_start < day_end).for_each(
            |t| {
                result.push(self.add_vat_markup(day_avg, t.entry_per_area.se4, t.delivery_start));
            });

        Ok(result)
    }

    /// Adds VAT and other markups such as energy taxes etc.
    ///
    /// # Arguments
    ///
    /// * 'day_avg' - average tariff for the day as from NordPool in SEK/MWh
    /// * 'tariff' - spot fee as from NordPool in SEK/MWh
    /// * 'delivery_start' - start time for the spot
    fn add_vat_markup(&self, day_avg: f64, tariff: f64, delivery_start: DateTime<Utc>) -> TariffValue {
        let price = tariff / 1000.0; // SEK per MWh to per kWh
        let grid_fees = (self.variable_fee + self.energy_tax) / 100.0 + self.spot_fee_percentage * day_avg;
        let trade_fees = (self.swedish_power_grid + self.balance_responsibility + self.electric_certificate +
            self.guarantees_of_origin + self.fixed) / 100.0 + price;

        let buy = (grid_fees + trade_fees) / 0.8;
        let sell = self.production_price / 100.0 + price;

        TariffValue {
            valid_time: delivery_start,
            price: round_to_two_decimals(price),
            buy: round_to_two_decimals(buy),
            sell: round_to_two_decimals(sell),
        }
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
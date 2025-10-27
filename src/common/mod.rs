use std::collections::HashMap;
use chrono::{DateTime, Local, Timelike};
use crate::common::models::{ForecastValue, ForecastValues, PowerValue, PowerValues};
use crate::errors::SchedulingError;
use crate::manager_production::errors::ProdError;
use crate::spline::MonotonicCubicSpline;

pub mod models;

impl PowerValues {
    /// Creates a new PowerValues struct from minute values
    ///
    /// # Arguments
    ///
    /// * 'data' - minute values over one full day
    /// * 'date_time' - date to use as a basis for result struct
    pub fn from_minute_values(data: [f64;1440], date_time: DateTime<Local>) -> PowerValues {
        let mut power = Vec::new();
        data.iter().enumerate().for_each(|(t, d)| {
            let dt = date_time.with_hour(t as u32 / 60u32).unwrap().with_minute(t as u32 % 60u32).unwrap();
            power.push(PowerValue {
                valid_time: dt,
                power: *d,
            });
        });

        PowerValues {power}
    }

    /// Transforms a day worth of power data from hourly to per minute
    ///
    /// # Arguments
    ///
    /// * 'forecast' - weather forecast assumed to be per hour
    pub fn minute_values(&self, date_time: DateTime<Local>) -> Result<[f64;1440], SchedulingError> {
        let xy = self.power
            .iter()
            .filter(|f| f.valid_time.date_naive() == date_time.date_naive())
            .map(|f| ((f.valid_time.hour() * 60 + f.valid_time.minute()) as f64, f.power))
            .collect::<Vec<(f64, f64)>>();
        let (x, y): (Vec<f64>, Vec<f64>) = xy.into_iter().unzip();
        let s = MonotonicCubicSpline::new(&x, &y)?;
        let mut power = [0.0; 1440];
        power.iter_mut().enumerate().for_each(|(i, t)| {
            *t = s.interpolate(i as f64);
        });

        Ok(power)
    }

    /// Returns a grouped version of the data input
    /// Data is grouped per `group` minutes, and the group function is average
    ///
    /// # Arguments
    ///
    /// * 'consumption' - data to be grouped
    /// * 'date_time' - date to use as a basis for result struct
    /// * 'group' - minutes per group from input data
    pub fn group_on_time(&self, date_time: DateTime<Local>, group: u32) -> Result<PowerValues, SchedulingError> {
        let data = self.minute_values(date_time)?;
        let grouped = group_minute_values(data, date_time, group);

        let mut power = Vec::new();
        grouped.into_iter().for_each(|(date, value)| {
            power.push(PowerValue {
                valid_time: date,
                power: value / (60.0 / group as f64),
            });
        });

        Ok(PowerValues {power})
    }
}

impl ForecastValues {
    /// Transforms a day worth if forecast values to minute values
    ///
    /// # Arguments
    ///
    /// * 'date_time' - date to transform
    /// * 'y_fn' - function that picks out whatever attribute to use from the forecast
    pub fn minute_values(&self, date_time: DateTime<Local>, y_fn: fn(&ForecastValue) -> f64) -> Result<[f64;1440], ProdError> {
        let xy = self.forecast
            .iter()
            .filter(|f| f.valid_time.date_naive() == date_time.date_naive())
            .map(|f| ((f.valid_time.hour() * 60 + f.valid_time.minute()) as f64, y_fn(f)))
            .collect::<Vec<(f64, f64)>>();
        let (x, y): (Vec<f64>, Vec<f64>) = xy.into_iter().unzip();
        let s = MonotonicCubicSpline::new(&x, &y)?;
        let mut temp = [0.0; 1440];
        temp.iter_mut().enumerate().for_each(|(i, t)| {
            *t = s.interpolate(i as f64);
        });

        Ok(temp)
    }
}

/// Groups minute values into a vector of time and values
///
/// # Arguments
///
/// * 'data' - minute values over one full day
/// * 'date_time' - 'date_time' - date to use as a basis for the result
/// * 'group' - minutes per group from input data
fn group_minute_values(data: [f64;1440], date_time: DateTime<Local>, group: u32) -> Vec<(DateTime<Local>, f64)> {
    let mut map: HashMap<u32, (f64, f64)> = HashMap::new();

    for (i, d) in data.iter().enumerate() {
        let _ = map
            .entry((i as u32 / group) * group)
            .and_modify(|v|{v.0 += *d; v.1 += 1.0;})
            .or_insert((*d, 1.0));
    }

    let mut grouped = map
        .into_iter()
        .map(|(t, v)| {
            let dt = date_time.with_hour(t / 60u32).unwrap().with_minute(t % 60u32).unwrap();
            (dt, v.0 / v.1)
        })
        .collect::<Vec<(DateTime<Local>, f64)>>();
    grouped.sort_by(|a, b| a.0.cmp(&b.0));

    grouped
}


use std::ops::Add;
use chrono::{Datelike, TimeDelta, Timelike};
use crate::config::ConsumptionParameters;
use crate::models::ForecastValues;
use crate::spline::MonotonicCubicSpline;


/// Struct for calculating the consumption load per hour given a weather forecast
///
pub struct Consumption {
    min_avg_load: f64,
    max_avg_load: f64,
    diagram: [[f64;24];7],
    curve_x_min: f64,
    curve_x_max: f64,
    curve: MonotonicCubicSpline,
}

impl Consumption {
    /// Returns a new Consumption struct
    ///
    /// # Arguments
    ///
    /// * 'config' - configuration struct
    pub fn new(config: &ConsumptionParameters) -> Consumption {
        let (curve_x, curve_y): (Vec<f64>, Vec<f64>) = config.curve
            .iter()
            .map(|c| (c.0, c.1))
            .unzip();

        Consumption { 
            min_avg_load: config.min_avg_load,
            max_avg_load: config.max_avg_load,
            diagram: config.diagram.unwrap(),
            curve_x_min: curve_x[0],
            curve_x_max: curve_x[curve_x.len() - 1],
            curve: MonotonicCubicSpline::new(&curve_x, &curve_y)
                .expect("Failed to create consumption curve"),
        }
    }
    
    /// Calculates hourly household consumption based on the temperature forecast and
    /// returns it as minute values in an array.
    ///
    /// Since all datetime values are to be in Utc, we need the current offset to compensate
    /// for the household diagram being in local time (it is given from how people act during
    /// a day regardless of the time zone or daylight saving time).
    ///
    /// # Arguments
    ///
    /// * 'forecast' - the temperature forecast
    /// * 'local_offset' - current offset between Utc and Local in seconds
    pub fn estimate(&self, forecast: &ForecastValues, local_offset: i64) -> Vec<f64> {
        let minutes = forecast.forecast.len() * 60;
        let mut p: Vec<f64> = vec![0.0;minutes];
        let mut minute_index = 0usize;

        for v in forecast.forecast.iter() {
            let valid_time = v.valid_time.add(TimeDelta::seconds(local_offset));
            let week_day = valid_time.weekday().num_days_from_monday() as usize;
            let hour = valid_time.hour() as usize;
            let power_per_hour = self.consumption_curve(v.temp) + self.diagram[week_day][hour];
            for i in minute_index..minute_index + 60 {
                p[i] = power_per_hour;
            }
            minute_index += 60;
        }

        p
    }

    /// Calculates consumption based on temperature over an estimated curve.
    /// The curve is formed such that it gives an approximation for house consumption within
    /// an outdoor temperature range. It is assumed that temperatures outside that range
    /// don't change much on the consumption in the climate of southern Sweden.
    ///
    /// Output varies between max_avg_load and min_avg_load
    ///
    /// # Arguments
    ///
    /// * 'temp' - outside temperature
    fn consumption_curve(&self, temp: f64) -> f64 {
        let capped_temp = temp.max(self.curve_x_min).min(self.curve_x_max);
        let curve = self.curve.interpolate(capped_temp).clamp(0.0, 1.0);

        curve * (self.max_avg_load - self.min_avg_load) + self.min_avg_load
    }
}



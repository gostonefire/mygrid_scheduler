use chrono::{DateTime, Datelike, Local, Timelike};
use anyhow::Result;
use crate::config::ConsumptionParameters;
use crate::common::models::{ForecastValues, TimeValue, TimeValues};
use crate::spline::MonotonicCubicSpline;


/// Struct for calculating the consumption load per hour given a weather forecast
///
/// The business logic is implemented in the calculate_consumption function. The current version is
/// just an inverse linear proportion between temperature and estimated load.
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
    
    /// Calculates hourly household consumption based on temperature forecast
    ///
    /// # Arguments
    ///
    /// * 'forecast' - the temperature forecast
    /// * 'date_time' - the date to calculate for
    pub fn estimate(&self, forecast: &ForecastValues, date_time: DateTime<Local>) -> Result<TimeValues> {
        let mut power: TimeValues = TimeValues::new(date_time);

        for v in forecast.forecast.iter().filter(|v| v.valid_time.date_naive() == date_time.date_naive()) {
            let week_day = v.valid_time.weekday().num_days_from_monday() as usize;
            let hour = v.valid_time.hour() as usize;
            let power_per_hour = self.consumption_curve(v.temp) + self.diagram[week_day][hour];
            power.push(TimeValue { valid_time: v.valid_time, data: power_per_hour })?;
        }

        Ok(power)
    }

    /// Calculates consumption based on temperature over an estimated curve.
    /// The curve is formed such that it gives an approximation for house consumption within
    /// an outdoor temperature range. It is assumed that temperatures outside that range
    /// don't change much on the consumption in the climate of southern Sweden.
    ///
    /// Output varies between MAX_AVG_LOAD and MIN_AVG_LOAD
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



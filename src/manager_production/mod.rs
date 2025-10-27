pub mod errors;

use std::collections::HashMap;
use std::ops::Add;
use chrono::{DateTime, DurationRound, Local, TimeDelta, Timelike};
use serde::Serialize;
use spa_sra::errors::SpaError;
use spa_sra::spa::{Function, Input, SpaData};
use crate::config::ProductionParameters;
use crate::manager_production::errors::ProdError;
use crate::common::models::{ForecastValue, ForecastValues, PowerValue, PowerValues};
use crate::spline::MonotonicCubicSpline;

/// Struct for calculating PV production based on solar positions and cloud conditions
///
pub struct PVProduction {
    lat: f64,
    long: f64,
    panel_power: f64,
    panel_slope: f64,
    panel_east_azm: f64,
    panel_temp_red: f64,
    tau: f64,
    tau_down: f64,
    k_gain: f64,
    iam_factor: f64,
    start_azm_elv: Vec<(f64, f64)>,
    stop_azm_elv: Vec<(f64, f64)>,
    cloud_impact_factor: f64,
}

impl PVProduction {
    /// Returns a new instance of PVProduction
    ///
    pub fn new(params: &ProductionParameters, lat: f64, long: f64) -> PVProduction {
        PVProduction {
            lat,
            long,
            panel_power: params.panel_power,
            panel_slope: params.panel_slope,
            panel_east_azm: params.panel_east_azm,
            panel_temp_red: params.panel_temp_red,
            tau: params.tau,
            tau_down: params.tau_down,
            k_gain: params.k_gain,
            iam_factor: params.iam_factor,
            start_azm_elv: params.start_azm_elv.clone(),
            stop_azm_elv: params.stop_azm_elv.clone(),
            cloud_impact_factor: params.cloud_impact_factor,
        }
    }

    /// Calculate estimates for the day included in the forecast vector.
    /// The result is an array of power per minute
    ///
    /// # Arguments
    ///
    /// * 'forecast' - a vector of hourly weather forecasts
    /// * 'date_time' - the date to calculate for
    pub fn estimate(&self, forecast: &ForecastValues, date_time: DateTime<Local>) -> Result<[f64;1440], ProdError> {
        let temp = forecast.minute_values(date_time, |f| f.temp)?;
        let cloud_factor = forecast.minute_values(date_time, |f| f.cloud_factor)?;
        let power_per_minute = self.day_power(date_time, temp, cloud_factor)?;
        
        Ok(power_per_minute)
    }

    /// Calculates one day estimated power per minute
    ///
    /// # Arguments
    ///
    /// * 'date_time' - date to calculate for
    /// * 'temp' - ambient temperature in degrees Celsius
    fn day_power(&self, date_time: DateTime<Local>, temp: [f64;1440], cloud_factor: [f64;1440]) -> Result<[f64;1440], ProdError> {
        let mut power: [f64;1440] = [0.0;1440];
        let sp = self.solar_positions(date_time)?;
        let sun_intensity_factor = sun_intensity_factor(&sp.zenith);
        let (up, down) = self.full_sun_minute(&sp);
        let roof_temperature_east: [f64;1440] = self.roof_temperature(Some(up), &temp, &sp.incidence_east, &sun_intensity_factor)?;
        let roof_temperature_west: [f64;1440] = self.roof_temperature(Some(up), &temp, &sp.incidence_west, &sun_intensity_factor)?;

        // Loop through the day with a one-minute incrementation
        for minute_of_day in sp.sunrise..sp.sunset {
            // Calculate factor on power production given sun incidence angles
            let inc_red_e = schlick_iam(sp.incidence_east[minute_of_day], self.iam_factor);
            let inc_red_w = schlick_iam(sp.incidence_west[minute_of_day], self.iam_factor);

            // Calculate power reduction due to high temperatures
            let temp_red_e = 1.0 - (roof_temperature_east[minute_of_day].max(0.0) - 25.0) * self.panel_temp_red / 100.0;
            let temp_red_w = 1.0 - (roof_temperature_west[minute_of_day].max(0.0) - 25.0) * self.panel_temp_red / 100.0;

            // Calculate power reduction due to the atmospheric effect given sun altitude relative to zenith
            let ame_red = sun_intensity_factor[minute_of_day];

            // Calculate total panel power where each side is reduced given the above power reduction factors
            let pwr = self.panel_power * 12.0 * inc_red_e * temp_red_e + self.panel_power * 15.0 * inc_red_w * temp_red_w;

            // Calculate the shadow factors for the given minute of the day
            let shadow_up = exp_increase(minute_of_day, sp.sunrise, up, 10);
            let shadow_down = exp_decrease(minute_of_day, down, sp.sunset, 4);

            // Calculate the cloud factor for the given minute of the day
            let cloud_factor = cloud_factor[minute_of_day].clamp(0.0, 1.0) * self.cloud_impact_factor + (1.0 - self.cloud_impact_factor);

            // Record the estimated power at the given point in time
            power[minute_of_day] = pwr * ame_red * shadow_up * shadow_down * cloud_factor;
        }

        Ok(power)
    }

    /// Returns sun incidence, zenith, azimuth and elevation angles per minute in degrees for the given date.
    ///
    /// # Arguments
    ///
    /// * 'date_time' - DateTime object carrying the date of interest
    fn solar_positions(&self, date_time: DateTime<Local>) -> Result<SolarPositions, SpaError> {
        let mut input = Input::from_date_time(date_time);
        input.latitude = self.lat;
        input.longitude = self.long;
        input.pressure = 1013.0;
        input.temperature = 10.0;
        input.elevation = 61.0;
        input.slope = self.panel_slope;
        input.azm_rotation = 0.0;
        input.function = Function::SpaZaRts;

        let mut spa = SpaData::new(input);
        spa.spa_calculate()?;

        let sunrise = spa.get_sunrise().duration_round(TimeDelta::minutes(1)).unwrap();
        let sunset = spa.get_sunset().duration_round(TimeDelta::minutes(1)).unwrap();

        spa.input.function = Function::SpaZaInc;

        let mut time_of_interest = sunrise;

        let mut incidence_east: [f64;1440] = [90.0; 1440];
        let mut incidence_west: [f64;1440] = [90.0; 1440];
        let mut zenith: [f64;1440] = [90.0; 1440];
        let mut azimuth: [f64;1440] = [0.0; 1440];
        let mut elevation: [f64;1440] = [0.0; 1440];

        while time_of_interest < sunset {
            spa.input.date_time(time_of_interest);
            let toi = (time_of_interest.hour() * 60 + time_of_interest.minute()) as usize;

            spa.input.azm_rotation = self.panel_east_azm;
            spa.spa_calculate()?;

            incidence_east[toi] = spa.spa_za_inc.incidence.min(90.0);
            zenith[toi] = spa.spa_za.zenith.clamp(0.0, 90.0);
            azimuth[toi] = spa.spa_za.azimuth;
            elevation[toi] = spa.spa_za.e.max(0.0);

            spa.input.azm_rotation = 180.0 + self.panel_east_azm;
            spa.spa_calculate()?;
            incidence_west[toi] = spa.spa_za_inc.incidence.min(90.0);

            time_of_interest = time_of_interest.add(TimeDelta::minutes(1));
        }


        Ok(SolarPositions {
            incidence_east,
            incidence_west,
            azimuth,
            elevation,
            zenith,
            sunrise: (sunrise.hour() * 60 + sunrise.minute()) as usize,
            sunset: (sunset.hour() * 60 + sunset.minute()) as usize,
        })
    }

    /// Finds the points in time (minute) where the sun is free from nearby obstacles
    ///
    /// # Arguments
    ///
    /// * 'solar_positions' - solar positions during the day
    fn full_sun_minute(&self, solar_positions: &SolarPositions) -> (usize, usize) {
        let mut up: usize = 0;
        let mut down: usize = 0;

        let mut up_pairs: Vec<(f64,f64,f64)> = Vec::new();
        let obst_len = self.start_azm_elv.len();
        for i in 0..obst_len {
            if i < obst_len - 1 {
                up_pairs.push((self.start_azm_elv[i].0, self.start_azm_elv[i+1].0, self.start_azm_elv[i].1));
            } else {
                up_pairs.push((self.start_azm_elv[i].0, 180.0, self.start_azm_elv[i].1));
                break;
            }
        }

        let mut down_pairs: Vec<(f64,f64,f64)> = Vec::new();
        let obst_len = self.stop_azm_elv.len();
        for i in 0..obst_len {
            if i < obst_len - 1 {
                down_pairs.push((self.stop_azm_elv[i].0, self.stop_azm_elv[i+1].0, self.stop_azm_elv[i].1));
            } else {
                down_pairs.push((self.stop_azm_elv[i].0, 360.0, self.stop_azm_elv[i].1));
                break;
            }
        }

        for m in solar_positions.sunrise..solar_positions.sunset {
            if solar_positions.azimuth[m] < 180.0 {
                for up_obst in up_pairs.iter() {
                    if up == 0 && solar_positions.azimuth[m] >= up_obst.0 && solar_positions.azimuth[m] < up_obst.1 && solar_positions.elevation[m] > up_obst.2 {
                        up = m;
                    }
                }
            } else {
                for down_obst in down_pairs.iter() {
                    if down == 0 && solar_positions.azimuth[m] >= down_obst.0 && solar_positions.azimuth[m] < down_obst.1 && solar_positions.elevation[m] < down_obst.2 {
                        down = m;
                    }
                }
            }
        }

        (up,down)
    }

    /// Calculates roof temperature given ambient temperature and effect from direct sunlight
    ///
    /// # Arguments
    ///
    /// * 'up' - time when the sun is free from obstacles
    /// * 'temp' - ambient temperature in degrees Celsius
    /// * 'inc_deg' - sun incidence on panels in degrees
    /// * 'sif' - sun intensity factor
    fn roof_temperature(&self, up: Option<usize>, temp: &[f64], inc_deg: &[f64;1440], sif: &[f64;1440]) -> Result<[f64;1440], ProdError> {

        let t_roof = roof_thermodynamics(
            temp,
            inc_deg,
            sif,
            60.0,
            self.tau * 3600.0,
            self.k_gain,
            None,
            None,
            Some(self.tau_down * 3600.0),
            up)?;

        let mut result: [f64;1440] = [0.0; 1440];
        (0..1440)
            .into_iter()
            .for_each(|i| {
                result[i] = t_roof[i];
            });

        Ok(result)
    }
}

/// Calculates an exponential increase for v between v0 and vn
/// The output is an exponential factor between 0 and 1
///
/// # Arguments
///
/// * 'v' - the input value
/// * 'v0' - the starting point for when v influences the output
/// * 'vn' - the end point for when v gives an output of 1 and no longer influences output
/// * 'exp' - exponent that determines the exponential shape
fn exp_increase(v: usize, v0: usize, vn: usize, exp: i32) -> f64 {
    let denominator = (vn - v0) as f64;
    let enumerator = (v as f64 - v0 as f64).clamp(0.0, denominator);

    (enumerator / denominator).powi(exp)
}

/// Calculates an exponential decrease for v between v0 and vn
/// The output is an exponential decrease factor between 1 and 0
///
/// # Arguments
///
/// * 'v' - the input value
/// * 'v0' - the starting point for when v influences the output
/// * 'vn' - the end point for when v gives an output of 1 and no longer influences output
/// * 'exp' - exponent that determines the exponential shape
fn exp_decrease(v: usize, v0: usize, vn: usize, exp: i32) -> f64 {
    let denominator = (vn - v0) as f64;
    let enumerator = (vn as f64 - v as f64).clamp(0.0, denominator);

    (enumerator / denominator).powi(exp)
}

/// The Schlick Incidence Angle Modifier algorithm
///
/// # Arguments
///
/// * 'theta_deg' - Sun-panel incidence angle
/// * 'factor' - level of flatness, 1 gives cosine flatness, higher values give more flatness
pub fn schlick_iam(theta_deg: f64, factor: f64) -> f64 {
    1.0 - (1.0 - theta_deg.clamp(0.0, 90.0).to_radians().cos()).powf(factor)
}

/// Returns percentage of sun intensity in relation to intensity external to the earth's atmosphere.
/// The algorithm (https://en.wikipedia.org/wiki/Air_mass_(solar_energy)) is based on the
/// air mass effect and then approximated to sun intensity.
///
/// # Arguments
///
/// * 'zenith_angle' - sun angle in relation to sun zenith (expected to be clamped between 0 and 90)
fn sun_intensity_factor(zenith_angle: &[f64;1440]) -> [f64;1440] {

    // The ratio between the earth's radius (6371 km) and the effective height of the atmosphere (9 km)
    const R: f64 = 708.0;

    // Intensity external to earths atmosphere
    const I_0: f64 = 1353.0;

    let mut result: [f64;1440] = [0.0; 1440];

    for i in 0..1440usize {
        let zenith_cos = zenith_angle[i].to_radians().cos();
        let enumerator = 2.0 * R + 1.0;
        let denominator = ((R * zenith_cos).powf(2.0) + 2.0 * R + 1.0).sqrt() + R * zenith_cos;
        let am = enumerator / denominator;

        // Approximation of sun intensity where the shape 0.6 replaces the originally proposed shape of 0.678
        let intensity = 1.1 * I_0 * 0.7f64.powf(am.powf(0.6));

        // Percentage of intensity compared to I_0
        result[i] = intensity / I_0;
    }

    result
}

/// Roof temperature over time using a 1st-order thermal RC model.
///
/// State update (explicit Euler):
///   T_roof[k] = T_roof[k-1] + (T_eq - T_roof[k-1]) * (dt / tau_eff)
/// where:
///   T_eq = T_air[k] + K * max(0, cos(inc_deg[k])) * clouds[k]
///   tau_eff = tau (when heating) or tau_down.unwrap_or(tau) (when cooling)
///
/// Notes:
/// - inc_deg is the sun incidence angle relative to the roof normal (0 deg = perpendicular to roof).
///   For a horizontal roof, inc_deg = 90 - altitude_deg.
/// - cos(inc_deg) gives the direct-beam projection onto the roof plane and is clamped at 0.
///
/// # Arguments
/// * `t_air`    : ambient air temperature [°C], length N
/// * `inc_deg`  : sun incidence angle to the roof normal [degrees], length N
/// * `sif`      : sun intensity factor, length N
/// * `dt`       : timestep [s], e.g. 600.0
/// * `tau`      : time constant for heating [s]
/// * `k_gain`   : °C boost at clear-sky normal incidence (proxy for A*α*G_max/U)
/// * `clouds`   : optional attenuation array in [0,1], length N (defaults to 1.0)
/// * `t0`       : optional initial roof temperature [°C] (defaults to t_air[0])
/// * `tau_down` : optional time constant for cooling [s] (defaults to `tau`)
///
/// # Returns
///
/// Vector of roof temperatures [°C], length N.
///
/// # Panics
///
/// Panics if input lengths mismatch or if `dt <= 0.0` or any tau ≤ 0.0.
fn roof_thermodynamics(
    t_air: &[f64],
    inc_deg: &[f64],
    sif: &[f64],
    dt: f64,
    tau: f64,
    k_gain: f64,
    clouds: Option<&[f64]>,
    t0: Option<f64>,
    tau_down: Option<f64>,
    up: Option<usize>,
) -> Result<Vec<f64>, ProdError> {
    let n = t_air.len();
    if n == 0 {
        return Ok(Vec::new());
    }

    // Check arrays lengths and input values
    if inc_deg.len() != n || sif.len() != n {
        return Err("inc_rad and sif must have the same length as t_air".into());
    }
    if let Some(c) = clouds {
        if c.len() != n {
            return Err("clouds must have the same length as t_air".into());
        }
    }
    if dt <= 0.0 {
        return Err("dt must be > 0".into());
    }
    if tau <= 0.0 {
        return Err("tau must be > 0".into());
    }
    if let Some(td) = tau_down {
        if td <= 0.0 {
            return Err("tau_down must be > 0".into());
        }
    }

    let mut t_roof = vec![0.0; n];
    let t_air_0 = if up.is_some() {
        t_air[0] - 4.0
    } else {
        t_air[0]
    };

    t_roof[0] = t0.unwrap_or(t_air_0);
    let tau_cool = tau_down.unwrap_or(tau);
    let up_delay = up.unwrap_or(0);

    for k in 1..n {
        // clouds[k] defaults to 1.0 if not provided
        let cloud_k = clouds.map_or(1.0, |c| c[k]);

        // Use projection by incidence: cos(inc_rad), clamped to [0, +inf) at 0.
        let inc_deg_k = if k <= up_delay {
            90.0
        } else {
            inc_deg[k]
        };

        let projection = inc_deg_k.to_radians().cos().max(0.0);
        let sun_boost = k_gain * projection * cloud_k; // [°C]

        let t_air_k = if k <= up_delay {
            t_air[k] - 4.0
        } else {
            t_air[k]
        };

        let t_eq = t_air_k + sun_boost * sif[k];

        let tau_eff = if t_eq > t_roof[k - 1] { tau } else { tau_cool };
        let alpha = dt / tau_eff; // Euler gain

        t_roof[k] = t_roof[k - 1] + (t_eq - t_roof[k - 1]) * alpha;
    }

    Ok(t_roof)
}

/// Transforms a day worth if forecast values to minute values
///
/// # Arguments
///
/// * 'forecast' - weather forecast assumed to be per hour
/// * 'date_time' - date to transform
/// * 'y_fn' - function that picks out whatever attribute to use from the forecast
fn minute_values(forecast: &Vec<ForecastValue>, date_time: DateTime<Local>, y_fn: fn(&ForecastValue) -> f64) -> Result<[f64;1440], ProdError> {
    let xy = forecast
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

/// Returns a grouped version of the data input
/// Data is grouped per `group` minutes, and the group function is average
///
/// # Arguments
///
/// * 'data' - data to be grouped
/// * 'date_time' - date to use as a basis for result struct
/// * 'group' - minutes per group from input data
fn group_on_time(data: [f64;1440], date_time: DateTime<Local>, group: u32) -> Vec<PowerValue> {
    let mut map: HashMap<u32, (f64, f64)> = HashMap::new();

    for (i, d) in data.iter().enumerate() {
        let _ = map
            .entry((i as u32 / group) * group)
            .and_modify(|v|{v.0 += *d; v.1 += 1.0;})
            .or_insert((*d, 1.0));
    }

    let mut result = map
        .into_iter()
        .map(|(t, v)| {
            let dt = date_time.with_hour(t / 60u32).unwrap().with_minute(t % 60u32).unwrap();
            PowerValue { valid_time: dt, power: v.0 / v.1 }
        })
        .collect::<Vec<PowerValue>>();
    result.sort_by(|a, b| a.valid_time.cmp(&b.valid_time));

    result
}

struct SolarPositions {
    incidence_east: [f64;1440],
    incidence_west: [f64;1440],
    azimuth: [f64;1440],
    elevation: [f64;1440],
    zenith: [f64;1440],
    sunrise: usize,
    sunset: usize,
}

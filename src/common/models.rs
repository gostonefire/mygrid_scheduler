use std::collections::HashMap;
use std::marker::PhantomData;
use chrono::{DateTime, Local, Timelike};
use serde::{Deserialize, Serialize};
use anyhow::Result;
use crate::errors::{TimeValuesError};
use crate::spline::MonotonicCubicSpline;

#[derive(Serialize, Debug)]
pub struct BaseData {
    pub date_time: DateTime<Local>,
    pub forecast: Vec<ForecastValue>,
    pub production: Vec<TimeValue>,
    pub consumption: Vec<TimeValue>,
    pub tariffs: Vec<TariffValue>,
}

#[derive(Serialize, Debug)]
pub struct TariffValue {
    pub valid_time: DateTime<Local>,
    pub price: f64,
    pub buy: f64,
    pub sell: f64,
}

#[derive(Clone, Serialize, Debug)]
pub struct TimeValue {
    pub valid_time: DateTime<Local>,
    pub data: f64
}

pub struct TimeValues {
    pub date_time: DateTime<Local>,
    pub data: Vec<TimeValue>,
    _marker: PhantomData<bool>,
}

pub struct MinuteValues {
    pub date_time: DateTime<Local>,
    pub data: [f64;1440],
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


impl MinuteValues {

    /// Creates a new MinuteValues
    /// 
    /// # Arguments
    /// 
    /// * 'data' - a day worth of data per minute
    /// * 'date_time' - the date the data is valid for
    pub fn new(data: [f64;1440], date_time: DateTime<Local>) -> MinuteValues {
        MinuteValues {data, date_time}
    }

    /// Returns a grouped version
    /// 
    /// Data is grouped per `group` minutes as an average, and the data is normalized to value per hour
    ///
    /// # Arguments
    ///
    /// * 'group' - minutes per group from input data
    pub fn time_groups(&self, group: u32) -> TimeValues {
        let grouped = self.group_minute_values(group);

        let mut data = Vec::new();
        grouped.into_iter().for_each(|(date, value)| {
            data.push(TimeValue {
                valid_time: date,
                data: value / (60.0 / group as f64),
            });
        });

        TimeValues {data, date_time: self.date_time, _marker: PhantomData }
    }

    /// Returns a TimeValues struct
    ///
    pub fn time_values(&self) -> TimeValues {
        let data: Vec<TimeValue> = self.data.iter().enumerate().map(|(i, d)| {
            let dt = self.date_time.with_hour(i as u32 / 60u32).unwrap().with_minute(i as u32 % 60u32).unwrap();
            TimeValue {
                valid_time: dt,
                data: *d,
            }
        }).collect();

        TimeValues {data, date_time: self.date_time, _marker: PhantomData }
    }

    /// Groups minute values into a vector of time and values
    ///
    /// # Arguments
    ///
    /// * 'data' - minute values over one full day
    /// * 'date_time' - 'date_time' - date to use as a basis for the result
    /// * 'group' - minutes per group from input data
    fn group_minute_values(&self, group: u32) -> Vec<(DateTime<Local>, f64)> {
        let mut map: HashMap<u32, (f64, f64)> = HashMap::new();

        for (i, d) in self.data.iter().enumerate() {
            let _ = map
                .entry((i as u32 / group) * group)
                .and_modify(|v|{v.0 += *d; v.1 += 1.0;})
                .or_insert((*d, 1.0));
        }

        let mut grouped = map
            .into_iter()
            .map(|(t, v)| {
                let dt = self.date_time.with_hour(t / 60u32).unwrap().with_minute(t % 60u32).unwrap();
                (dt, v.0 / v.1)
            })
            .collect::<Vec<(DateTime<Local>, f64)>>();
        grouped.sort_by(|a, b| a.0.cmp(&b.0));

        grouped
    }
}

impl TimeValues {

    /// Creates a new TimeValues struct bounded to the given date
    pub fn new(date_time: DateTime<Local>) -> TimeValues {
        TimeValues {data: Vec::new(), date_time, _marker: PhantomData }
    }

    pub fn push(&mut self, data: TimeValue) -> Result<()> {
        if data.valid_time.date_naive() != self.date_time.date_naive() {
            return Err(TimeValuesError::TimeValue)?;
        }
        self.data.push(data);

        Ok(())
    }

    /// Creates a new TimeValues struct from minute values
    ///
    /// # Arguments
    ///
    /// * 'data' - minute values over one full day
    /// * 'date_time' - date to use as a basis for result struct
    pub fn from_minute_values(data: [f64;1440], date_time: DateTime<Local>) -> TimeValues {
        let data: Vec<TimeValue> = data.iter().enumerate().map(|(i, d)| {
            let dt = date_time.with_hour(i as u32 / 60u32).unwrap().with_minute(i as u32 % 60u32).unwrap();
            TimeValue {
                valid_time: dt,
                data: *d,
            }
        }).collect();

        TimeValues {data, date_time, _marker: PhantomData }
    }

    /// Transforms a day worth of power data from hourly to per minute
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the date to represent the day
    pub fn minute_values(&self) -> Result<MinuteValues> {
        let xy = self.data
            .iter()
            .map(|f| ((f.valid_time.hour() * 60 + f.valid_time.minute()) as f64, f.data))
            .collect::<Vec<(f64, f64)>>();
        let (x, y): (Vec<f64>, Vec<f64>) = xy.into_iter().unzip();
        let s = MonotonicCubicSpline::new(&x, &y)?;
        let mut data = [0.0; 1440];
        data.iter_mut().enumerate().for_each(|(i, t)| {
            *t = s.interpolate(i as f64);
        });

        Ok(MinuteValues::new(data, self.date_time))
    }
}

impl ForecastValues {
    /// Transforms a day worth if forecast values to minute values
    ///
    /// # Arguments
    ///
    /// * 'date_time' - date to transform
    /// * 'y_fn' - function that picks out whatever attribute to use from the forecast
    pub fn minute_values(&self, date_time: DateTime<Local>, y_fn: fn(&ForecastValue) -> f64) -> Result<[f64;1440]> {
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

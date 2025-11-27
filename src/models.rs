use std::collections::HashMap;
use std::marker::PhantomData;
use std::ops::Add;
use chrono::{DateTime, TimeDelta, Utc};
use serde::Serialize;
use anyhow::Result;
use crate::errors::ForecastValuesError;
use crate::spline::MonotonicCubicSpline;

#[derive(Serialize, Debug)]
pub struct BaseData {
    pub date_time: DateTime<Utc>,
    pub base_cost: f64,
    pub schedule_cost: f64,
    pub forecast: Vec<ForecastValue>,
    pub production: Vec<TimeValue>,
    pub consumption: Vec<TimeValue>,
    pub tariffs: Vec<TariffValue>,
}

#[derive(Serialize, Debug)]
pub struct TariffValue {
    pub valid_time: DateTime<Utc>,
    pub price: f64,
    pub buy: f64,
    pub sell: f64,
}

#[derive(Clone, Serialize, Debug)]
pub struct TimeValue {
    pub valid_time: DateTime<Utc>,
    pub data: f64
}

pub struct TimeValues {
    pub data: Vec<TimeValue>,
    _marker: PhantomData<bool>,
}

pub struct MinuteValues {
    pub date_time: DateTime<Utc>,
    pub data: [f64;1440],
}

#[derive(Serialize, Debug)]
pub struct ForecastValue {
    pub valid_time: DateTime<Utc>,
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
    /// * 'date_time' - the date and time the data starts with
    pub fn new(data: [f64;1440], date_time: DateTime<Utc>) -> MinuteValues {
        MinuteValues {data, date_time}
    }

    /// Returns a grouped version
    /// 
    /// Data is grouped per `group` minutes as an average, and the data can be integrated to value hour
    ///
    /// # Arguments
    ///
    /// * 'group' - minutes per group from input data
    /// * 'integrate' - whether to convert to unit hour or just average within each group
    pub fn time_groups(&self, group: u32, integrate: bool) -> TimeValues {
        let grouped = self.group_minute_values(group);

        let mut data = Vec::new();
        grouped.into_iter().for_each(|(date, value)| {
            data.push(TimeValue {
                valid_time: date,
                data: if integrate { value / (60.0 / group as f64) } else { value },
            });
        });

        TimeValues {data, _marker: PhantomData }
    }

    /// Groups minute values into a vector of time and values
    ///
    /// # Arguments
    ///
    /// * 'group' - minutes per group from input data
    fn group_minute_values(&self, group: u32) -> Vec<(DateTime<Utc>, f64)> {
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
                let dt = self.date_time.add(TimeDelta::minutes(t as i64));
                (dt, v.0 / v.1)
            })
            .collect::<Vec<(DateTime<Utc>, f64)>>();
        grouped.sort_by(|a, b| a.0.cmp(&b.0));

        grouped
    }
}



impl ForecastValues {
    /// Transforms a day worth if forecast values to minute values starting from the first forecast value
    ///
    /// # Arguments
    ///
    /// * 'y_fn' - a function that picks out whatever attribute to use from the forecast
    pub fn minute_values(&self, y_fn: fn(&ForecastValue) -> f64) -> Result<[f64;1440]> {
        let base_minute = self.forecast.first().ok_or(ForecastValuesError::EmptyForecastValues)?.valid_time.timestamp() / 60;
        
        let xy = self.forecast
            .iter()
            .map(|f| ((f.valid_time.timestamp() / 60 - base_minute) as f64, y_fn(f)))
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

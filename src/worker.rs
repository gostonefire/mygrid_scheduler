use std::{fs, thread};
use std::ops::Add;
use chrono::{DateTime, Duration, DurationRound, Local, NaiveDate, NaiveDateTime, TimeDelta, Timelike, Utc};
use glob::glob;
use log::info;
use anyhow::Result;
use thiserror::Error;
use crate::config::{Config, Files};
use crate::initialization::Mgr;
use crate::models::{BaseData, MinuteValues};
use crate::{retry, wrapper};
use crate::scheduler::{Block, Schedule, SchedulerResult};

/// Runs a schedule creation process
///
/// # Arguments
///
/// * 'config' - configuration
/// * 'mgr' - struct with configured managers
/// * 'files' - files config
/// * 'debug_run_time' - a run start date and time to be used instead of Local now
/// * 'debug_soc_in' - a soc in to be used instead of whatever is calculated
pub fn run(config: &Config, mgr: &mut Mgr, files: &Files, debug_run_time: Option<DateTime<Local>>, debug_soc_in: Option<u8>) -> Result<(), WorkerError> {

    // If a run time is given, use that. Otherwise, use the current time.
    let run_start = if let Some(run_start) = debug_run_time {
        run_start
    } else {
        Local::now()
    };

    let run_schema = get_schedule_start_schema(run_start)?;

    info!("Run start: {}, Schedule Start: {}", run_schema.run_start, run_schema.schedule_start);

    // Estimate how much battery capacity we lose between the run start and the schedule start
    let start_soc = if let Some(soc_in) = debug_soc_in {
        soc_in
    } else {
        estimate_soc_in(mgr, &run_schema, config.charge.soc_kwh)?
    };

    // Calculate the new schedule
    let (scheduler_result, base_data) = get_schedule(&config, mgr, start_soc, &run_schema)?;

    info!("Base Cost: {}, Schedule Cost: {}", scheduler_result.base_cost, scheduler_result.total_cost);
    for b in scheduler_result.blocks.iter() {
        info!("{}", b);
    }

    save_schedule(&files.schedule_dir, scheduler_result.start_time, scheduler_result.end_time, &scheduler_result.blocks)?;
    save_base_data(&files.base_data_dir, &base_data)?;

    Ok(())
}

/// Tries to estimate what the SoC will be at a specific time
///
/// # Arguments
///
/// * 'mgr' - struct with managers
/// * 'run_schema' - a schema with a schedule for running the scheduler, and time converted to Utc
/// * 'soc_kwh' - kwh per soc unit
fn estimate_soc_in(mgr: &mut Mgr, run_schema: &RunSchema, soc_kwh: f64) -> Result<u8, WorkerError> {
    // Get the current state of charge from Fox Cloud
    let soc_in = retry!(||mgr.fox.get_current_soc())
        .map_err(|e| WorkerError::EstimateSocError(format!("error getting current soc: {}", e.to_string())))?;


    // Loop through all the dates and calculate the power used during the day and sum it up
    let mut power_used = 0f64;
    let mut minutes: usize = 0;

    {
        // Calculate production and consumption during the day that run_start falls into
        let forecast = retry!(||mgr.forecast.new_forecast(run_schema.run_day_start, run_schema.run_day_end))
            .map_err(|e| WorkerError::EstimateSocError(format!("error getting forecast: {}", e.to_string())))?;
        let production = mgr.pv.estimate(&forecast, run_schema.run_day_start, run_schema.run_day_end, run_schema.run_date)
            .map_err(|e| WorkerError::EstimateSocError(format!("error estimating production: {}", e.to_string())))?;
        let consumption = mgr.cons.estimate(&forecast, run_schema.local_offset);

        // Calculate the power used in the time span
        // The power used is divided by the number of minutes to get the average power used per hour
        power_used += (run_schema.run_date_1.run_start_minute..run_schema.run_date_1.run_end_minute).fold(0f64, |acc, i|
            acc + (production[i] - consumption[i])) / 60.0 / 1000.0;

        minutes += run_schema.run_date_1.run_end_minute - run_schema.run_date_1.run_start_minute;
    }

    if let Some(schedule_date) = &run_schema.run_date_2 {
        // Calculate production and consumption in the time span between schedule day start and schedule start
        let forecast = retry!(||mgr.forecast.new_forecast(run_schema.schedule_day_start, run_schema.schedule_day_end))
            .map_err(|e| WorkerError::EstimateSocError(format!("error getting forecast: {}", e.to_string())))?;
        let production = mgr.pv.estimate(&forecast, run_schema.schedule_day_start, run_schema.schedule_day_end, run_schema.schedule_date)
            .map_err(|e| WorkerError::EstimateSocError(format!("error estimating production: {}", e.to_string())))?;
        let consumption = mgr.cons.estimate(&forecast, run_schema.local_offset);

        // Calculate the power used in the time span
        // The power used is divided by the number of minutes to get the average power used per hour
        power_used += (schedule_date.run_start_minute..schedule_date.run_end_minute).fold(0f64, |acc, i|
            acc + (production[i] - consumption[i])) / 60.0 / 1000.0;

        minutes += schedule_date.run_end_minute - schedule_date.run_start_minute;
    }

    // Calculate the expected SoC at the start of the schedule.
    // 10% is the lowest SoC that the battery accepts; anything below is considered 10%
    let start_soc = (soc_in as i8 + (power_used / soc_kwh) as i8).max(10) as u8;
    info!("Soc In: {}, Start SoC: {}, Power Used: {}, Minutes: {}", soc_in, start_soc, power_used, minutes);
    Ok(start_soc)
}

/// Calculates a new schedule
///
/// # Arguments
///
/// * 'config' - configuration
/// * 'mgr' - struct with managers
/// * 'soc_in' - state of battery charge when going in to the schedule
/// * 'run_schema' - a schema with a schedule for running the scheduler, and time converted to Utc
fn get_schedule(config: &Config, mgr: &mut Mgr, soc_in: u8, run_schema: &RunSchema) -> Result<(SchedulerResult, BaseData), WorkerError> {
    let forecast = retry!(||mgr.forecast.new_forecast(run_schema.schedule_day_start, run_schema.schedule_day_end))
        .map_err(|e| WorkerError::GetScheduleError(format!("error getting forecast: {}", e.to_string())))?;
    let pv_estimate = mgr.pv.estimate(&forecast, run_schema.schedule_day_start, run_schema.schedule_day_end, run_schema.schedule_date)
        .map_err(|e| WorkerError::GetScheduleError(format!("error estimating production: {}", e.to_string())))?;
    let cons_estimate = mgr.cons.estimate(&forecast, run_schema.local_offset);

    let production = MinuteValues::new(&pv_estimate, run_schema.schedule_day_start).time_groups(15, true);
    let consumption = MinuteValues::new(&cons_estimate, run_schema.schedule_day_start).time_groups(15, true);
    let tariffs = retry!(||mgr.nordpool.get_tariffs(run_schema.schedule_day_start, run_schema.schedule_day_end, run_schema.schedule_date))
        .map_err(|e| WorkerError::GetScheduleError(format!("error getting tariffs: {}", e.to_string())))?;

    let mut scheduler = Schedule::new(config);
    let pd = Schedule::preformat_data(&tariffs, &production.data, &consumption.data, run_schema.schedule_start, run_schema.schedule_day_end)
        .map_err(|e| WorkerError::GetScheduleError(format!("error preformatting data: {}", e.to_string())))?;
    info!("Time blocks to schedule for: {}", pd.tariffs.len());

    let sr = scheduler.update_scheduling(&pd.tariffs, &pd.cons, &pd.net_prod, soc_in, run_schema.schedule_start);

    let base_data = BaseData {
        date_time: run_schema.schedule_day_start,
        base_cost: sr.base_cost,
        schedule_cost: sr.total_cost,
        production: MinuteValues::new(&pv_estimate, run_schema.schedule_day_start).time_groups(5, false).data,
        consumption: MinuteValues::new(&cons_estimate, run_schema.schedule_day_start).time_groups(5, false).data,
        forecast: forecast.forecast,
        tariffs,
    };

    Ok((sr, base_data))
}

/// Creates a run schema to be used to calculate the SoC at the time of schedule start
///
/// # Arguments
///
/// * 'run_start' - time when calculation starts
fn get_schedule_start_schema(run_start: DateTime<Local>) -> Result<RunSchema, WorkerError> {
    // The run start is given, the schedule start, however, is assumed to be some x minutes
    // in the future since it takes quite a while to calculate.
    //
    // The rules are:
    // * If the run starts before 21:00, we calculate a schedule for the rest of the current day.
    // * if the run starts at or after 23:15, we add one hour and cannibalize from the next day (1395 is the minute of the day for 23:15).
    // * Otherwise, we calculate a schedule for the entire next day
    let schedule_start = if run_start.hour() < 21 || run_start.hour() * 60 + run_start.minute() >= 1395 {
        run_start
            .add(TimeDelta::hours(1))
            .duration_trunc(TimeDelta::minutes(15))
            .map_err(|e| WorkerError::RunSchemaError(format!("run_start date: {}", e.to_string())))?
    } else {
        run_start
            .with_hour(12).unwrap()
            .add(TimeDelta::days(1))
            .duration_trunc(TimeDelta::hours(1))
            .map_err(|e| WorkerError::RunSchemaError(format!("run_start date: {}", e.to_string())))?
            .with_hour(0).unwrap()
    };

    let run_start_utc = run_start.with_timezone(&Utc);
    let (run_day_start_utc, run_day_end_utc) = get_utc_day_start(run_start_utc, 0);
    let run_date_naive = run_start.date_naive();

    let schedule_start_utc = schedule_start.with_timezone(&Utc);
    let (schedule_day_start_utc, schedule_day_end_utc) = get_utc_day_start(schedule_start_utc, 0);
    let schedule_date_naive = schedule_start.date_naive();

    // Calculate the time span between the run start and the schedule start
    // This can involve one or two days, depending on whether the run start falls into the same day as the schedule start
    let mut run_date_1: RunMinutes = RunMinutes {
        run_start_minute: 1440 - (run_day_end_utc - run_start_utc).num_minutes() as usize,
        run_end_minute: 1440,
    };
    let mut run_date_2: Option<RunMinutes> = None;

    if schedule_start_utc < run_day_end_utc {
        run_date_1.run_end_minute = 1440 - (run_day_end_utc - schedule_start_utc).num_minutes() as usize;
    } else if schedule_start_utc >  run_day_end_utc {
        run_date_2 = Some(RunMinutes {
            run_start_minute: 0,
            run_end_minute: 1440 - (schedule_day_end_utc - schedule_start_utc).num_minutes() as usize,
        });
    };

    Ok(RunSchema {
        run_start: run_start_utc,
        run_day_start: run_day_start_utc,
        run_day_end: run_day_end_utc,
        run_date: run_date_naive,
        schedule_start: schedule_start_utc,
        schedule_day_start: schedule_day_start_utc,
        schedule_day_end: schedule_day_end_utc,
        schedule_date: schedule_date_naive,
        local_offset: run_start.offset().local_minus_utc() as i64,
        run_date_1,
        run_date_2,
    })
}

/// Saves a schedule to file for consumption
///
/// # Arguments
///
/// * 'path' - path to the schedule directory
/// * 'schedule_start' - the time when the schedule starts
/// * 'schedule_end' - the time when the schedule ends (non-inclusive)
/// * 'schedule' - the vector of block that represents the schedule
fn save_schedule(path: &str, schedule_start: DateTime<Utc>, schedule_end: DateTime<Utc>, schedule: &Vec<Block>) -> Result<(), WorkerError> {
    let filename = format!("{}{}_{}_schedule.json", path, schedule_start.format("%Y%m%d%H%M"), schedule_end.format("%Y%m%d%H%M"));

    let json = serde_json::to_string_pretty(schedule)
        .map_err(|e| WorkerError::SaveScheduleError(format!("error serializing schedule: {}", e.to_string())))?;

    fs::write(&filename, json)
        .map_err(|e| WorkerError::SaveScheduleError(format!("error writing schedule to file: {}", e.to_string())))?;

    clean_up_files(&format!("{}*_schedule.json", path), schedule_start)?;

    info!("Schedule saved to {}", filename);

    Ok(())
}

/// Saves base data for use in e.g. MyGridDash
///
/// # Arguments
///
/// * 'path' - path to the base data dir
/// * 'base_data' - base data to save
fn save_base_data(path: &str, base_data: &BaseData) -> Result<(), WorkerError> {
    let filename = format!("{}{}_base_data.json", path, base_data.date_time.format("%Y%m%d%H%M"));

    let json = serde_json::to_string_pretty(base_data)
        .map_err(|e| WorkerError::SaveBaseDataError(format!("error serializing base data: {}", e.to_string())))?;

    fs::write(&filename, json)
        .map_err(|e| WorkerError::SaveBaseDataError(format!("error writing base data to file: {}", e.to_string())))?;

    clean_up_files(&format!("{}*_base_data.json", path), base_data.date_time)?;

    info!("Backup data saved to {}", filename);

    Ok(())
}

/// Removes any files following the pattern that are older than 48 hours
///
/// # Arguments
///
/// * 'pattern' - file pattern
/// * 'gate_date_time' - the date time representing a newly created file
fn clean_up_files(pattern: &str, gate_date_time: DateTime<Utc>) -> Result<(), WorkerError> {
    for entry in glob(&pattern)
        .map_err(|e| WorkerError::CleanUpError(format!("error reading files with pattern {}: {}", pattern, e.to_string())))? {
        if let Ok(path) = entry {
            if let Some(os_name) = path.file_name() {
                if let Some(filename) = os_name.to_str() {
                    let datetime: DateTime<Utc> = NaiveDateTime::parse_from_str(&filename[0..12], "%Y%m%d%H%M")
                        .map_err(|e| WorkerError::CleanUpError(format!("error parsing date: {}", e.to_string())))?
                        .and_local_timezone(Utc).unwrap();
                    if gate_date_time - datetime > Duration::hours(48) {
                        fs::remove_file(path)
                            .map_err(|e| WorkerError::CleanUpError(format!("error removing file: {}", e.to_string())))?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Returns the start and end (non-inclusive) of a day in UTC time.
/// For DST switch days (summer to winter time and vice versa), the length of the day
/// will be either 23 hours (in the spring) or 25 hours (in the autumn).
///
/// # Arguments
///
/// * 'date_time' - date time to get utc day start and end for (in relation to Local timezone)
/// * 'day_index' - 0-based index of the day, 0 is today, -1 is yesterday, etc.
fn get_utc_day_start(date_time: DateTime<Utc>, day_index: i64) -> (DateTime<Utc>, DateTime<Utc>) {
    // First, go local and move hour to a safe place regarding DST day shift between summer and winter time.
    // Also, apply the day index to get to the desired day.
    let date = date_time.with_timezone(&Local).with_hour(12).unwrap().add(TimeDelta::days(day_index));

    // Then trunc to a whole hour and move time to the start of day local (Chrono manages offset change if necessary)
    let start = date.duration_trunc(TimeDelta::hours(1)).unwrap().with_hour(0).unwrap();

    // Then add one day and do the same as for start
    let end = date.add(TimeDelta::days(1)).duration_trunc(TimeDelta::hours(1)).unwrap().with_hour(0).unwrap();

    (start.with_timezone(&Utc), end.with_timezone(&Utc))
}

struct RunMinutes {
    run_start_minute: usize,
    run_end_minute: usize,
}

struct RunSchema {
    run_start: DateTime<Utc>,
    run_day_start: DateTime<Utc>,
    run_day_end: DateTime<Utc>,        // Non-inclusive
    run_date: NaiveDate,
    schedule_start: DateTime<Utc>,
    schedule_day_start: DateTime<Utc>,
    schedule_day_end: DateTime<Utc>,   // Non-Inclusive
    schedule_date: NaiveDate,
    local_offset: i64,
    run_date_1: RunMinutes,
    run_date_2: Option<RunMinutes>,
}

/// Error depicting errors that occur while running the scheduler
///
#[derive(Debug, Error)]
#[error("error while running scheduler")]
pub enum WorkerError {
    #[error("error while creating run schema: {0:?}")]
    RunSchemaError(String),
    #[error("error while saving schedule: {0:?}")]
    SaveScheduleError(String),
    #[error("error while saving base data: {0:?}")]
    SaveBaseDataError(String),
    #[error("error while cleaning up old files: {0:?}")]
    CleanUpError(String),
    #[error("error while estimating soc: {0:?}")]
    EstimateSocError(String),
    #[error("error while getting schedule: {0:?}")]
    GetScheduleError(String),
}

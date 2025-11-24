use std::{fs, thread};
use std::ops::Add;
use chrono::{DateTime, Duration, DurationRound, Local, NaiveDateTime, TimeDelta, Timelike, Utc};
use rayon::ThreadPoolBuilder;
use anyhow::Result;
use glob::glob;
use log::{error, info};
use crate::common::models::{BaseData, MinuteValues};
use crate::config::Files;
use crate::errors::SchedulingError;
use crate::initialization::{init, Mgr};
use crate::scheduler::Block;

mod scheduler;
mod manager_nordpool;
mod manager_production;
mod spline;
mod errors;
mod manager_mail;
mod manager_forecast;
mod manager_fox_cloud;
mod config;
mod initialization;
mod consumption;
mod logging;
mod macros;
mod common;

fn main() -> Result<()> {
    ThreadPoolBuilder::new().num_threads(2).build_global()?;
    
    // Load config and set up all managers. If initialization fails, we are pretty much out of luck
    // and can't even log or send notification mail.
    let (config, mut mgr) = match init() {
        Ok((c, m)) => (c, m),
        Err(e) => {
            return Err(SchedulingError(format!("Initialization failed: {}", e)))?;
        }   
    };

    // Create a new schedule
    match run(&mut mgr, &config.files) {
        Ok(_) => {
            mgr.mail.send_mail("Report".into(), "Successfully created new schedule".into())?;
        },
        Err(e) => {
            error!("Run failed: {}", e);
            mgr.mail.send_mail("Error in scheduler".into(), format!("Run failed: {}", e))?;
            return Err(e)?;
        }
    }

    Ok(())
}

/// Runs a schedule creation process
///
/// # Arguments
///
/// * 'mgr' - struct with configured managers
/// * 'files' - files config
fn run(mgr: &mut Mgr, files: &Files) -> Result<()> {
    let run_start = DateTime::parse_from_rfc3339("2025-11-23T23:00:00+01:00")?.with_timezone(&Local);

    // The run start is always assumed to be at call of this function, the schedule start, however,
    // is assumed to be some x minutes in the future since it takes quite a while to calculate.
    // The rules are:
    // * If the run starts before 21:00, we calculate a schedule for the rest of the current day.
    // * if the run starts at or after 23:15, we add one hour and cannibalize from the next day (1395 is the minute of the day for 23:15).
    // * Otherwise, we calculate a schedule for the entire next day
    //let run_start = Local::now();
    let run_schema = get_schedule_start_schema(run_start)?;
    dbg!(&run_schema);
    info!("Run start: {}, Schedule Start: {}", run_schema.run_start, run_schema.schedule_start);

    // Estimate how much battery capacity we lose between the run start and the schedule start
    let start_soc = estimate_soc_in(mgr, &run_schema)?;

    // Calculate the new schedule
    let base_data = get_schedule(mgr, start_soc, &run_schema)?;

    info!("Base Cost: {}, Schedule Cost: {}", mgr.schedule.base_cost, mgr.schedule.total_cost);
    for b in mgr.schedule.blocks.iter() {
        info!("{}", b);
    }

    save_schedule(&files.schedule_dir, mgr.schedule.start_time, mgr.schedule.end_time, &mgr.schedule.blocks)?;
    save_base_data(&files.base_data_dir, &base_data)?;

    Ok(())
}

/// Tries to estimate what the SoC will be at a specific time
/// 
/// # Arguments
/// 
/// * 'mgr' - struct with managers
/// * 'run_start' - time when calculation starts
/// * 'schedule_start' - time when the new schedule is expected to start
/// * 'offset' - current offset between Utc and Local time
fn estimate_soc_in(mgr: &mut Mgr, run_schema: &RunSchema) -> Result<u8> {
    // Get the current state of charge from Fox Cloud
    let soc_in = retry!(||mgr.fox.get_current_soc())?;


    // Loop through all the dates and calculate the power used during the day and sum it up
    let mut power_used = 0f64;
    let mut minutes: usize = 0;

    {
        // Calculate production and consumption during the day that run_start falls into
        let forecast = retry!(||mgr.forecast.new_forecast(run_schema.run_day_start, run_schema.run_day_end))?;
        let production = mgr.pv.estimate(&forecast, run_schema.run_day_start, run_schema.run_date)?;
        let consumption = mgr.cons.estimate(&forecast, run_schema.local_offset)?;

        // Calculate the power used in the time span between the run start and the schedule start
        // The power used is divided by the number of minutes to get the average power used per hour
        power_used += (run_schema.run_date_1.run_start_minute..run_schema.run_date_1.run_end_minute).fold(0f64, |acc, i|
            acc + (production[i] - consumption[i])) / 1000.0;

        minutes += run_schema.run_date_1.run_end_minute - run_schema.run_date_1.run_start_minute;
    }

    if let Some(schedule_date) = &run_schema.run_date_2 {
        // Calculate production and consumption during the day that run_start falls into
        let forecast = retry!(||mgr.forecast.new_forecast(run_schema.run_day_start, run_schema.run_day_end))?;
        let production = mgr.pv.estimate(&forecast, run_schema.schedule_day_start, run_schema.schedule_date)?;
        let consumption = mgr.cons.estimate(&forecast, run_schema.local_offset)?;

        // Calculate the power used in the time span between the run start and the schedule start
        power_used += (schedule_date.run_start_minute..schedule_date.run_end_minute).fold(0f64, |acc, i|
            acc + (production[i] - consumption[i])) / 1000.0;

        minutes += schedule_date.run_end_minute - schedule_date.run_start_minute;
    }

    // Calculate the expected SoC at the start of the schedule.
    // 10% is the lowest SoC that the battery accepts; anything below is considered 10%
    let start_soc = (soc_in as i8 + (power_used / mgr.schedule.get_soc_kwh()) as i8).max(10) as u8;
    info!("Soc In: {}, Start SoC: {}, Power Used: {}, Minutes: {}", soc_in, start_soc, power_used, minutes);
    Ok(start_soc)
}

/// Calculates a new schedule
///
/// # Arguments
///
/// * 'mgr' - struct with managers
/// * 'run_start' - time when calculation starts
/// * 'schedule_start' - time when the new schedule is expected to start
fn get_schedule(mgr: &mut Mgr, soc_in: u8, run_schema: &RunSchema) -> Result<BaseData> {
    let forecast = retry!(||mgr.forecast.new_forecast(run_schema.schedule_day_start, run_schema.schedule_day_end))?;
    let pv_estimate = mgr.pv.estimate(&forecast, run_schema.schedule_day_start, run_schema.schedule_date)?;
    let cons_estimate = mgr.cons.estimate(&forecast, run_schema.local_offset)?;

    let production = MinuteValues::new(pv_estimate, run_schema.schedule_day_start).time_groups(15);
    let consumption = MinuteValues::new(cons_estimate, run_schema.schedule_day_start).time_groups(15);
    let tariffs = retry!(||mgr.nordpool.get_tariffs(run_schema.schedule_date))?;

    mgr.schedule.update_scheduling(&tariffs, &production.data, &consumption.data, soc_in, run_schema.schedule_start, run_schema.schedule_length);

    let base_data = BaseData {
        date_time: run_schema.schedule_start,
        base_cost: mgr.schedule.base_cost,
        schedule_cost: mgr.schedule.total_cost,
        production: MinuteValues::new(pv_estimate, run_schema.schedule_day_start).time_groups(5).data,
        consumption: MinuteValues::new(cons_estimate, run_schema.schedule_day_start).time_groups(5).data,
        forecast: forecast.forecast,
        tariffs,
    };

    Ok(base_data)
}

/// Creates a run schema to be used to calculate the SoC at the time of schedule start
///
/// # Arguments
///
/// * 'run_start' - time when calculation starts
fn get_schedule_start_schema(run_start: DateTime<Local>) -> Result<RunSchema> {
    // The run start is always assumed to be at call of this function, the schedule start, however,
    // is assumed to be some x minutes in the future since it takes quite a while to calculate.
    // The rules are:
    // * If the run starts before 21:00, we calculate a schedule for the rest of the current day.
    // * if the run starts at or after 23:15, we add one hour and cannibalize from the next day (1395 is the minute of the day for 23:15).
    // * Otherwise, we calculate a schedule for the entire next day
    let schedule_start = if run_start.hour() < 21 || run_start.hour() * 60 + run_start.minute() >= 1395 {
        run_start.add(TimeDelta::hours(1)).duration_trunc(TimeDelta::minutes(15))?
    } else {
        run_start.add(TimeDelta::days(1)).duration_trunc(TimeDelta::days(1))?
    };
    let run_start_utc = run_start.with_timezone(&Utc);
    let run_day_start_utc = run_start.duration_trunc(TimeDelta::days(1))?.with_timezone(&Utc);
    let run_day_end_utc = run_day_start_utc.add(TimeDelta::days(1));
    let run_date_utc = run_start.with_hour(12).unwrap().with_timezone(&Utc).duration_trunc(TimeDelta::days(1))?;

    let schedule_start_utc = schedule_start.with_timezone(&Utc);
    let schedule_day_start_utc = schedule_start.duration_trunc(TimeDelta::days(1))?.with_timezone(&Utc);
    let schedule_day_end_utc = schedule_day_start_utc.add(TimeDelta::days(1));
    let schedule_date_utc = schedule_start.with_hour(12).unwrap().with_timezone(&Utc).duration_trunc(TimeDelta::days(1))?;

    let schedule_length = 24 * 60 - (schedule_start_utc - schedule_day_start_utc).num_minutes();

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
        run_date: run_date_utc,
        schedule_start: schedule_start_utc,
        schedule_day_start: schedule_day_start_utc,
        schedule_day_end: schedule_day_end_utc,
        schedule_length,
        schedule_date: schedule_date_utc,
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
fn save_schedule(path: &str, schedule_start: DateTime<Utc>, schedule_end: DateTime<Utc>, schedule: &Vec<Block>) -> Result<()> {
    let filename = format!("{}{}_{}_schedule.json", path, schedule_start.format("%Y%m%d%H%M"), schedule_end.format("%Y%m%d%H%M"));

    let json = serde_json::to_string_pretty(schedule)?;

    fs::write(&filename, json)?;

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
fn save_base_data(path: &str, base_data: &BaseData) -> Result<()> {
    let filename = format!("{}{}_base_data.json", path, base_data.date_time.format("%Y%m%d%H%M"));

    let json = serde_json::to_string_pretty(base_data)?;

    fs::write(&filename, json)?;

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
fn clean_up_files(pattern: &str, gate_date_time: DateTime<Utc>) -> Result<()> {
    for entry in glob(&pattern)? {
        if let Ok(path) = entry {
            if let Some(os_name) = path.file_name() {
                if let Some(filename) = os_name.to_str() {
                    let datetime: DateTime<Utc> = NaiveDateTime::parse_from_str(&filename[0..12], "%Y%m%d%H%M")?.and_local_timezone(Utc).unwrap();
                    if gate_date_time - datetime > Duration::hours(48) {
                        fs::remove_file(path)?;
                    }
                }
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
struct RunMinutes {
    run_start_minute: usize,
    run_end_minute: usize,
}

#[derive(Debug)]
struct RunSchema {
    run_start: DateTime<Utc>,
    run_day_start: DateTime<Utc>,
    run_day_end: DateTime<Utc>,        // Non-inclusive
    run_date: DateTime<Utc>,
    schedule_start: DateTime<Utc>,
    schedule_day_start: DateTime<Utc>,
    schedule_day_end: DateTime<Utc>,   // Non-Inclusive
    schedule_length: i64,
    schedule_date: DateTime<Utc>,
    local_offset: i64,
    run_date_1: RunMinutes,
    run_date_2: Option<RunMinutes>,
}

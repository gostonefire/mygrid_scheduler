use std::thread;
use std::ops::Add;
use chrono::{DateTime, DurationRound, Local, NaiveTime, TimeDelta, Timelike};
use rayon::ThreadPoolBuilder;
use anyhow::Result;
use log::{error, info};
use toml::value::Time;
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
    ThreadPoolBuilder::new().num_threads(4).build_global()?;
    
    // Load config and set up all managers. If initialization fails, we are pretty much out of luck
    // and can't even log or send notification mail.
    let (config, mut mgr) = match init() {
        Ok((c, m)) => (c, m),
        Err(e) => {
            return Err(SchedulingError(format!("Initialization failed: {}", e)))?;
        }   
    };

    match run(&mut mgr) {
        Ok(_) => (),
        Err(e) => {
            error!("Run failed: {}", e);
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
fn run(mgr: &mut Mgr) -> Result<()> {
    //let run_start = DateTime::parse_from_rfc3339("2025-10-30T20:59:00+01:00")?.with_timezone(&Local);

    // The run start is always assumed to be at call of this function, the schedule start, however,
    // is assumed to be some x minutes in the future since it takes quite a while to calculate.
    // The rules are:
    // * If the run starts before 21:00, we calculate a schedule for the rest of the current day.
    // * if the run starts at or after 23:15, we add one hour and cannibalize from the next day (1395 is the minute of the day for 23:15).
    // * Otherwise, we calculate a schedule for the entire next day
    let run_start = Local::now();
    let schedule_start = if run_start.hour() < 21 || run_start.hour() * 60 + run_start.minute() >= 1395 {
        run_start.add(TimeDelta::hours(1)).duration_trunc(TimeDelta::minutes(15))?
    } else {
        run_start.add(TimeDelta::days(1)).duration_trunc(TimeDelta::days(1))?
    };
    info!("Run start: {}, Schedule Start: {}", run_start, schedule_start);

    // Estimate how much battery capacity we lose between the run start and the schedule start
    let start_soc = estimate_soc_in(mgr, run_start, schedule_start)?;

    // Calculate the new schedule
    let blocks = get_schedule(mgr, start_soc, schedule_start)?;

    info!("Base Cost: {}, Schedule Cost: {}", mgr.schedule.base_cost, mgr.schedule.total_cost);
    for b in blocks.iter() {
        println!("{:?}", b);
    }
    Ok(())
}

/// Tries to estimate what the SoC will be at a specific time
/// 
/// # Arguments
/// 
/// * 'mgr' - struct with managers
/// * 'run_start' - time when calculation starts
/// * 'schedule_start' - time when the new schedule is expected to start
fn estimate_soc_in(mgr: &mut Mgr, run_start: DateTime<Local>, schedule_start: DateTime<Local>) -> Result<u8> {
    if schedule_start.date_naive() < run_start.date_naive() {
        return Err(SchedulingError("Schedule start is in the past".to_string()))?;
    }

    //calculate production and consumption during the day that run_start falls into
    let forecast = retry!(||mgr.forecast.new_forecast(run_start))?;
    let production = mgr.pv.estimate(&forecast, run_start)?;
    let consumption = mgr.cons.estimate(&forecast, run_start)?.minute_values()?;


    // Get the current state of charge from Fox Cloud
    let soc_in = retry!(||mgr.fox.get_current_soc())?;

    // Calculate the span in minutes between the run start and the schedule start
    // If the schedule start is the next day, it is assumed to be midnight (00:00)
    let minute_start = (run_start.hour() * 60 + run_start.minute()) as usize;
    let minute_end = if schedule_start.date_naive() == run_start.date_naive() {
        (schedule_start.hour() * 60 + schedule_start.minute()) as usize
    } else {
        1440
    };

    // Calculate the power used in the time span between the run start and the schedule start
    // The power used is divided by the number of minutes to get the average power used per hour
    let power_used = (minute_start..minute_end).fold(0f64, |acc, i|
        acc + (production.data[i] - consumption.data[i])) / 1000.0 / 60.0;

    // Calculate the expected SoC at the start of the schedule.
    // 10% is the lowest SoC that the battery accepts; anything below is considered 10%
    let start_soc = (soc_in as i8 + (power_used / mgr.schedule.get_soc_kwh()) as i8).max(10) as u8;
    info!("Soc In: {}, Start SoC: {}, Power Used: {}, Minutes: {}", soc_in, start_soc, power_used, minute_end - minute_start);
    Ok(start_soc)
}

/// Calculates a new schedule
///
/// # Arguments
///
/// * 'mgr' - struct with managers
/// * 'run_start' - time when calculation starts
/// * 'schedule_start' - time when the new schedule is expected to start
fn get_schedule(mgr: &mut Mgr, soc_in: u8, schedule_start: DateTime<Local>) -> Result<Vec<Block>> {
    let forecast = retry!(||mgr.forecast.new_forecast(schedule_start))?;

    let production = mgr.pv.estimate(&forecast, schedule_start)?.time_groups(15);
    let consumption = mgr.cons.estimate(&forecast, schedule_start)?.minute_values()?.time_groups(15);

    let tariffs = retry!(||mgr.nordpool.get_tariffs(schedule_start))?;

    mgr.schedule.update_scheduling(&tariffs, &production.data, &consumption.data, soc_in, schedule_start);

    Ok(mgr.schedule.blocks.clone())
}
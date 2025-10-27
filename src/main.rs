use std::thread;
use std::ops::Add;
use std::sync::RwLock;
use chrono::{DateTime, DurationRound, Local, TimeDelta, Timelike};
use rayon::ThreadPoolBuilder;
use crate::errors::SchedulingError;
use crate::initialization::{init, Mgr};
use crate::common::models::PowerValues;
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

static LOGGER_INITIALIZED: RwLock<bool> = RwLock::new(false);

fn main() -> Result<(), SchedulingError> {
    ThreadPoolBuilder::new().num_threads(4).build_global().unwrap();
    
    // Load config and set up all managers. If initialization fails, we are pretty much out of luck
    // and can't even log or send notification mail.
    let (config, mut mgr) = match init() {
        Ok((c, m)) => (c, m),
        Err(e) => {
            println!("Initialization failed: {}", e);
            return Err(SchedulingError(format!("Initialization failed: {}", e)));
        }   
    };
    

    //let tariffs = tariffs_to_15_minute_step(data.tariffs);
    //let production = production_to_15_minute_step(data.production);
    //let consumption = consumption_to_15_minute_step(data.consumption);
    let run_start = Local::now();
    let schedule_start = Local::now().add(TimeDelta::days(1)).duration_trunc(TimeDelta::days(1)).unwrap();
    println!("Run start: {}, Schedule Start: {}", run_start, schedule_start);

    let start_soc = estimate_soc_in(&mut mgr, run_start, schedule_start)?;

    let blocks = get_schedule(&mut mgr, start_soc, schedule_start)?;

    println!("{:?}", blocks);

    //let mut s = Schedule::new(None);
    //s.update_scheduling(&data.tariffs, &data.production, &data.consumption, 4.48, data.date_time);
    //s.update_scheduling(&tariffs, &production, &consumption, 10, data.date_time);

    /*
    println!("Total cost: {}", s.total_cost);
    println!("{:?}", s.tariffs);
    for b in s.blocks.iter() {
        println!("{:?}", b);
    }
    
     */
    
    Ok(())
}

/// Tries to estimate what the SoC will be at a specific time
/// 
/// # Arguments
/// 
/// * 'mgr' - struct with managers
/// * 'run_start' - time when calculation starts
/// * 'schedule_start' - time when the new schedule is expected to start
fn estimate_soc_in(mgr: &mut Mgr, run_start: DateTime<Local>, schedule_start: DateTime<Local>) -> Result<u8, SchedulingError> {
    if schedule_start.date_naive() < run_start.date_naive() {
        return Err(SchedulingError("Schedule start is in the past".to_string()));
    }

    //calculate production and consumption during the day that run_start falls into
    let forecast = retry!(||mgr.forecast.new_forecast(run_start))?;
    let production = mgr.pv.estimate(&forecast, run_start)?;
    let consumption = mgr.cons.estimate(&forecast, run_start).minute_values(run_start)?;

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
        acc + (production[i] - consumption[i])) / 1000.0 / 60.0;

    // Calculate the expected SoC at the start of the schedule.
    // 10% is the lowest SoC that the battery accepts; anything below is considered 10%
    let start_soc = (soc_in as i8 + (power_used / mgr.schedule.get_soc_kwh()) as i8).max(10) as u8;
    println!("Soc In: {}, Start SoC: {}, Power Used: {}, Minutes: {}", soc_in, start_soc, power_used, minute_end - minute_start);
    Ok(start_soc)
}

fn get_schedule(mgr: &mut Mgr, soc_in: u8, schedule_start: DateTime<Local>) -> Result<Vec<Block>, SchedulingError> {
    let forecast = retry!(||mgr.forecast.new_forecast(schedule_start))?;

    let production = PowerValues::from_minute_values(mgr.pv.estimate(&forecast, schedule_start)?, schedule_start)
        .group_on_time(schedule_start, 15)?;

    let consumption = mgr.cons.estimate(&forecast, schedule_start)
        .group_on_time(schedule_start, 15)?;

    let tariffs = retry!(||mgr.nordpool.get_tariffs(schedule_start))?;

    mgr.schedule.update_scheduling(&tariffs, &production.power, &consumption.power, soc_in, schedule_start);

    println!("Base Cost: {}, Total Cost: {}", mgr.schedule.base_cost, mgr.schedule.total_cost);
    
    Ok(mgr.schedule.blocks.clone())
}
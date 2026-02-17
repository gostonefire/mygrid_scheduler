use rayon::ThreadPoolBuilder;
use anyhow::Result;
use log::error;
use thiserror::Error;
use crate::initialization::init;
use crate::worker::run;

mod scheduler;
mod manager_nordpool;
mod manager_production;
mod spline;
mod manager_mail;
mod manager_forecast;
// mod manager_fox_cloud;
mod config;
mod initialization;
mod consumption;
mod logging;
mod macros;
pub mod models;
mod worker;

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
    match run(&config, &mut mgr, &config.files, config.general.debug_run_time, config.general.debug_soc_soh_in) {
        Ok(_) => {
            mgr.mail.send_mail("Report".into(), "Successfully created new schedule".into())?;
        },
        Err(e) => {
            error!("Run failed: {}", e.to_string());
            mgr.mail.send_mail("Error in scheduler".into(), format!("Run failed: {}", e.to_string()))?;
            return Err(e)?;
        }
    }

    Ok(())
}

/// Error depicting errors that occur while creating and managing schedules
///
#[derive(Debug, Error)]
#[error("SchedulingError: {0}")]
pub struct SchedulingError(pub String);

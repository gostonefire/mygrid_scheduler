use std::env;
use log::info;
use anyhow::Result;
use thiserror::Error;
use crate::config::{load_config, Config, LoadConfigurationError};
use crate::consumption::Consumption;
use crate::logging::{setup_logger, LoggerError};
use crate::manager_forecast::Forecast;
use crate::manager_fox_cloud::Fox;
use crate::manager_mail::{Mail, MailError};
use crate::manager_nordpool::NordPool;
use crate::manager_production::PVProduction;

pub struct Mgr {
    pub fox: Fox,
    pub nordpool: NordPool,
    pub forecast: Forecast,
    pub pv: PVProduction,
    pub cons: Consumption,
    pub mail: Mail,
}

/// Initializes and returns configuration and a Mgr struct holding various of initialized structs
///
pub fn init() -> Result<(Config, Mgr), InitializationError> {
    let args: Vec<String> = env::args().collect();
    let config_path = args.iter()
        .find(|p| p.starts_with("--config="))
        .expect("config file argument should be present");
    let config_path = config_path
        .split_once('=')
        .expect("config file argument should be correct")
        .1;


    // Load configuration
    let config = load_config(&config_path)?;

    // Setup logging
    let _ = setup_logger(&config.general.log_path, config.general.log_level, config.general.log_to_stdout)?;


    // Print version
    info!("starting mygrid scheduler version: {}", env!("CARGO_PKG_VERSION"));

    
    // Instantiate structs
    let fox = Fox::new(&config.fox_ess);
    let nordpool = NordPool::new(&config.tariff_fees);
    let smhi = Forecast::new(&config);
    let pv = PVProduction::new(&config.production, config.geo_ref.lat, config.geo_ref.long);
    let cons = Consumption::new(&config.consumption);
    let mail = Mail::new(&config.mail)?;

    let mgr = Mgr {
        fox,
        nordpool,
        forecast: smhi,
        pv,
        cons,
        mail,
    };
 
    Ok((config, mgr))
}

/// Error depicting errors that occur while initializing the scheduler
///
#[derive(Debug, Error)]
pub enum InitializationError {
    #[error("ConfigurationError: {0}")]
    ConfigurationError(#[from] LoadConfigurationError),
    #[error("SetupLoggerError: {0}")]
    SetupLoggerError(#[from] LoggerError),
    #[error("MailSetupError: {0}")]
    MailSetupError(#[from] MailError),
}

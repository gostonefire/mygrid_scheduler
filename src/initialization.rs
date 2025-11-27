use std::env;
use log::info;
use anyhow::Result;
use crate::config::{load_config, Config};
use crate::consumption::Consumption;
use crate::logging::setup_logger;
use crate::manager_forecast::Forecast;
use crate::manager_fox_cloud::Fox;
use crate::manager_mail::Mail;
use crate::manager_nordpool::NordPool;
use crate::manager_production::PVProduction;
use crate::scheduler::Schedule;

pub struct Mgr {
    pub fox: Fox,
    pub nordpool: NordPool,
    pub forecast: Forecast,
    pub pv: PVProduction,
    pub cons: Consumption,
    pub mail: Mail,
    pub schedule: Schedule,
}

/// Initializes and returns configuration and a Mgr struct holding various of initialized structs
///
pub fn init() -> Result<(Config, Mgr)> {
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
    let schedule = Schedule::new(&config, None);

    let mgr = Mgr {
        fox,
        nordpool,
        forecast: smhi,
        pv,
        cons,
        mail,
        schedule,
    };
 
    Ok((config, mgr))
}
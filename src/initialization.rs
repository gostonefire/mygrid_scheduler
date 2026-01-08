use std::{env, fs};
use std::path::PathBuf;
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
    let mut config = load_config(&config_path)?;
    config.fox_ess.api_key = read_credential("fox_ess_api_key")?;
    config.fox_ess.inverter_sn = read_credential("fox_ess_inverter_sn")?;
    config.mail.smtp_user = read_credential("mail_smtp_user")?;
    config.mail.smtp_password = read_credential("mail_smtp_password")?;

    if config.general.debug_dir.is_some() {
        config.files.schedule_dir = config.general.debug_dir.clone().unwrap();
        config.files.base_data_dir = config.general.debug_dir.clone().unwrap();
    }
    
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

/// Reads a credential from the file system supported by the credstore and
/// given from systemd
///
/// # Arguments
///
/// * 'name' - name of the credential to read
fn read_credential(name: &str) -> Result<String, InitializationError> {
    let dir = env::var("CREDENTIALS_DIRECTORY")?;
    let mut p = PathBuf::from(dir);
    p.push(name);
    let bytes = fs::read(p)?;
    Ok(String::from_utf8(bytes)?.trim_end().to_string())
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
    #[error("CredentialFileError: {0}")]
    CredentialFileError(#[from] std::io::Error),
    #[error("CredentialEnvError: {0}")]
    CredentialEnvError(#[from] env::VarError),
    #[error("CredentialUtf8Error: {0}")]
    CredentialUtf8Error(#[from] std::string::FromUtf8Error),
}

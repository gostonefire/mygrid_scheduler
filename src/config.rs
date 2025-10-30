use std::fs;
use log::LevelFilter;
use serde::Deserialize;
use anyhow::Result;

#[derive(Deserialize)]
pub struct GeoRef {
    pub lat: f64,
    pub long: f64,
}

#[derive(Deserialize)]
pub struct ConsumptionParameters {
    pub min_avg_load: f64,
    pub max_avg_load: f64,
    pub curve: Vec<(f64, f64)>,
    #[serde(skip)]
    pub diagram: Option<[[f64;24];7]>,
}

#[derive(Deserialize)]
pub struct ProductionParameters {
    pub panel_power: f64,
    pub panel_slope: f64,
    pub panel_east_azm: f64,
    pub panel_temp_red: f64,
    pub tau: f64,
    pub tau_down: f64,
    pub k_gain: f64,
    pub iam_factor: f64,
    pub start_azm_elv: Vec<(f64, f64)>,
    pub stop_azm_elv: Vec<(f64, f64)>,
    pub cloud_impact_factor: f64,
    pub low_clouds_factor: f64,
    pub mid_clouds_factor: f64,
    pub high_clouds_factor: f64,
}
#[derive(Deserialize)]
pub struct ChargeParameters {
    pub bat_kwh: f64,
    pub soc_kwh: f64,
    pub charge_kwh_hour: f64,
    pub charge_efficiency: f64,
    pub discharge_efficiency: f64,
}

#[derive(Deserialize)]
pub struct FoxESS {
    pub api_key: String,
    pub inverter_sn: String,
}

#[derive(Deserialize)]
pub struct Forecast {
    pub host: String,
    pub port: u16,
}

#[derive(Deserialize)]
pub struct MailParameters {
    pub smtp_user: String,
    pub smtp_password: String,
    pub smtp_endpoint: String,
    pub from: String,
    pub to: String,
}

#[derive(Deserialize)]
pub struct Files {
    pub backup_dir: String,
    pub cons_diagram: String,
}

#[derive(Deserialize)]
pub struct General {
    pub log_path: String,
    pub log_level: LevelFilter,
    pub log_to_stdout: bool,
}

#[derive(Deserialize)]
pub struct Config {
    pub geo_ref: GeoRef,
    pub consumption: ConsumptionParameters,
    pub production: ProductionParameters,
    pub charge: ChargeParameters,
    pub fox_ess: FoxESS,
    pub forecast: Forecast,   
    pub mail: MailParameters,
    pub files: Files,
    pub general: General,
}

#[derive(Deserialize)]
struct DaysDiagram {
    monday: [f64; 24],
    tuesday: [f64; 24],
    wednesday: [f64; 24],
    thursday: [f64; 24],
    friday: [f64; 24],
    saturday: [f64; 24],
    sunday: [f64; 24],
}

#[derive(Deserialize)]
struct HouseHoldConsumption {
    consumption_diagram: DaysDiagram
}

/// Loads the configuration file and returns a struct with all configuration items
/// 
/// # Arguments
/// 
/// * 'config_path' - path to the configuration file
pub fn load_config(config_path: &str) -> Result<Config> {
    
    let toml = fs::read_to_string(config_path)?;
    let mut config: Config = toml::from_str(&toml)?;
    
    let cons_diagram = load_consumption_diagram(&config.files.cons_diagram)?;
    config.consumption.diagram = Some(cons_diagram);
    
    Ok(config)
}

/// Loads consumption diagram configuration
///
/// # Arguments
///
/// * 'diagram_path' - path to the consumption diagram file
fn load_consumption_diagram(diagram_path: &str) -> Result<[[f64;24];7]> {
    
    let toml = fs::read_to_string(diagram_path)?;
    let hhc: HouseHoldConsumption = toml::from_str(&toml)?;
    
    let days: [[f64;24];7] = [
        hhc.consumption_diagram.monday,
        hhc.consumption_diagram.tuesday,
        hhc.consumption_diagram.wednesday,
        hhc.consumption_diagram.thursday,
        hhc.consumption_diagram.friday,
        hhc.consumption_diagram.saturday,
        hhc.consumption_diagram.sunday];

        Ok(days)
}
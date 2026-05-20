pub mod models;

use std::time::Duration;
use reqwest::blocking::Client;
use thiserror::Error;
use crate::manager_inverter::models::{DataRecord};

pub struct Inverter {
    client: Client,
    host: String,
}

impl Inverter {
    pub fn new(host: &str) -> Result<Inverter, InverterError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client,
            host: host.to_string(),
        })
    }

    /// Asynchronously retrieves the state of charge (SOC) of the battery.
    ///
    pub fn get_soc(&self) -> Result<u8, InverterError> {
        Ok(self.get_data("battery_soc")? as u8)
    }

    /// Asynchronously retrieves the state of health (SOH) of the battery.
    ///
    pub fn get_soh(&self) -> Result<u8, InverterError> {
        Ok(self.get_data("battery_soh")? as u8)
    }

    /// Requests data from the inverter.
    ///
    /// # Arguments
    ///
    /// * `register_id` - The ID of the register to fetch data from.
    fn get_data(&self, register_id: &str) -> Result<f64, InverterError> {
        let url = format!("http://{}/id/{}", self.host, register_id);
        let req = self.client.get(&url).send()?;
        
        let status = req.status();
        if !status.is_success() {
            return Err(InverterError::InverterError(format!("response with status: {:?}", status)));
        }
        
        let json = req.text()?;
        let soc: DataRecord<f64> = serde_json::from_str(&json)?;
        
        Ok(soc.data)
    }
}


#[derive(Error, Debug)]
pub enum InverterError {
    #[error("NetworkError: {0}")]
    NetworkError(#[from] reqwest::Error),
    #[error("JsonError: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("InverterError: {0}")]
    InverterError(String),
}
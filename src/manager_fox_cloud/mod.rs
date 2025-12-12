pub mod errors;
mod models;

use std::str::FromStr;
use std::time::Duration;
use chrono::Utc;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use ureq::Agent;
use ureq::http::{HeaderMap, HeaderName, HeaderValue};
use anyhow::Result;
use crate::config::FoxESS;
use crate::manager_fox_cloud::errors::FoxError;
use crate::manager_fox_cloud::models::{RequestCurrentSoc, SocCurrentResult};

const REQUEST_DOMAIN: &str = "https://www.foxesscloud.com";

pub struct Fox {
    api_key: String,
    sn: String,
    agent: Agent,
}

impl Fox {
    /// Returns a new instance of the Fox struct
    ///
    /// # Arguments
    ///
    /// * 'api_key' - API key for communication with Fox Cloud
    /// * 'sn' - the serial number of the inverter to manage
    pub fn new(config: &FoxESS) -> Self {
        let agent_config = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build();

        let agent = agent_config.into();

        Self { api_key: config.api_key.to_string(), sn: config.inverter_sn.to_string(), agent }
    }

    /// Get the battery current soc (state of charge)
    ///
    /// See https://www.foxesscloud.com/public/i18n/en/OpenApiDocument.html#get20device20real-time20data0a3ca20id3dget20device20real-time20data4303e203ca3e
    ///
    /// # Arguments
    ///
    pub fn get_current_soc(&self) -> Result<u8, FoxError> {
        let path = "/op/v0/device/real/query";

        let req = RequestCurrentSoc { sn: self.sn.clone(), variables: vec!["SoC".to_string()] };
        let req_json = serde_json::to_string(&req)
            .map_err(|e| FoxError::GetSocError(format!("error serializing request: {}", e.to_string())))?;

        let json = self.post_request(&path, req_json)?;

        let fox_data: SocCurrentResult = serde_json::from_str(&json)
            .map_err(|e| FoxError::GetSocError(format!("error deserializing response: {}", e.to_string())))?;

        Ok(fox_data.result[0].datas[0].value.round() as u8)
    }


    /// Builds a request and sends it as a POST.
    /// The return is the JSON representation of the result as specified by
    ///  the respective FoxESS API
    ///
    /// # Arguments
    ///
    /// * path - the API path excluding the domain
    /// * body - a string containing the payload in JSON format
    fn post_request(&self, path: &str, body: String) -> Result<String, FoxError> {
        let url = format!("{}{}", REQUEST_DOMAIN, path);

        let mut req = self.agent.post(url);
        let headers = req.headers_mut().ok_or(FoxError::PostRequestError("request builder error".to_string()))?;
        self.generate_headers(headers, &path, Some(vec!(("Content-Type", "application/json"))));

        let json = req
            .send(body)
            .map_err(|e| FoxError::PostRequestError(format!("ureq error: {}", e.to_string())))?
            .body_mut()
            .read_to_string()
            .map_err(|e| FoxError::PostRequestError(format!("ureq error: {}", e.to_string())))?;

        let fox_res: FoxResponse = serde_json::from_str(&json)
            .map_err(|e| FoxError::PostRequestError(format!("error deserializing response: {}", e.to_string())))?;
        
        if fox_res.errno != 0 {
            return Err(FoxError::PostRequestError(format!("errno: {}, msg: {}", fox_res.errno, fox_res.msg)))?;
        }

        Ok(json)
    }

    /// Generates http headers required by Fox Open API; this includes also building a
    /// md5 hashed signature.
    ///
    /// # Arguments
    ///
    /// * 'headers' - a header map to insert new headers into
    /// * 'path' - the path, excluding the domain part, to the FoxESS specific API
    /// * 'extra' - any extra headers to add besides FoxCloud standards
    fn generate_headers(&self, headers: &mut HeaderMap, path: &str, extra: Option<Vec<(&str, &str)>>) {
        let timestamp = Utc::now().timestamp() * 1000;
        let signature = format!("{}\\r\\n{}\\r\\n{}", path, self.api_key, timestamp);

        let mut hasher = Md5::new();
        hasher.update(signature.as_bytes());
        let signature_md5 = hasher.finalize().iter().map(|x| format!("{:02x}", x)).collect::<String>();

        headers.insert("token", HeaderValue::from_str(&self.api_key).unwrap());
        headers.insert("timestamp", HeaderValue::from_str(&timestamp.to_string()).unwrap());
        headers.insert("signature", HeaderValue::from_str(&signature_md5).unwrap());
        headers.insert("lang", HeaderValue::from_str("en").unwrap());

        if let Some(h) = extra {
            h.iter().for_each(|&(k, v)| {
                headers.insert(HeaderName::from_str(k).unwrap(), HeaderValue::from_str(v).unwrap());
            });
        }
    }
}

#[derive(Serialize, Deserialize)]
struct FoxResponse {
    errno: u32,
    msg: String,
}



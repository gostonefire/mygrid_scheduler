use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct RequestCurrentBatState {
    pub variables: Vec<String>,
    pub sns: Vec<String>,}


#[derive(Deserialize)]
pub struct DeviceRealTimeResult {
    pub result: Vec<RealTimeVariables>,
}

#[derive(Deserialize)]
pub struct RealTimeVariables {
    pub datas: Vec<RealTimeData>,
}

#[derive(Deserialize)]
pub struct RealTimeData {
    pub variable: String,
    pub value: f64,
}

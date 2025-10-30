use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct RequestCurrentSoc {
    pub sn: String,
    pub variables: Vec<String>,
}

#[derive(Deserialize)]
pub struct SocCurrentData {
    pub value: f64,
}

#[derive(Deserialize)]
pub struct SocCurrentVariables {
    pub datas: Vec<SocCurrentData>,
}

#[derive(Deserialize)]
pub struct SocCurrentResult {
    pub result: Vec<SocCurrentVariables>,
}

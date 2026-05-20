use serde::Deserialize;

#[derive(Deserialize)]
pub struct DataRecord<T> {
    pub data: T,
}

use std::fs;
use crate::models::BackupData;
use crate::scheduling::Schedule;

mod scheduling;
mod models;

fn main() {
    let data = load_data("C:/Slask/mygrid_scheduling/20251011220032_base_data.json");
    let mut s = Schedule::new();
    s.update_scheduling(&data.tariffs, &data.production, &data.consumption, 4.48, data.date_time);
    
    println!("Total cost: {}", s.total_cost);
    for b in s.blocks {
        println!("{:?}", b);
    }
}

fn load_data(path: &str) -> BackupData {
    let json = fs::read_to_string(path).unwrap();
    let data: BackupData = serde_json::from_str(&json).unwrap();
    
    data
}
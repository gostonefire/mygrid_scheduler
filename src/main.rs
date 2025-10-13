use std::{env, fs};
use crate::models::BackupData;
use crate::scheduling::Schedule;

mod scheduling;
mod models;

fn main() {
    let args: Vec<String> = env::args().collect();
    let config_path = args.iter()
        .find(|p| p.starts_with("--config="))
        .expect("config file argument should be present");
    let config_path = config_path
        .split_once('=')
        .expect("config file argument should be correct")
        .1;

    let data = load_data(config_path);
    let mut s = Schedule::new();
    //s.update_scheduling(&data.tariffs, &data.production, &data.consumption, 4.48, data.date_time);
    s.update_scheduling(&data.tariffs, &data.production, &data.consumption, 0.0, data.date_time);

    println!("Total cost: {}", s.total_cost);
    println!("{:?}", s.tariffs);
    for b in s.blocks.iter() {
        println!("{:?}", b);
    }
}

fn load_data(path: &str) -> BackupData {
    let json = fs::read_to_string(path).unwrap();
    let data: BackupData = serde_json::from_str(&json).unwrap();
    
    data
}
use std::{env, fs};
use std::ops::Add;
use chrono::TimeDelta;
use rayon::ThreadPoolBuilder;
use crate::models::{BackupData, ConsumptionValues, ProductionValues, TariffValues};
use crate::scheduler::Schedule;

mod scheduler;
mod models;

fn main() {
    ThreadPoolBuilder::new().num_threads(2).build_global().unwrap();
    
    let args: Vec<String> = env::args().collect();
    let config_path = args.iter()
        .find(|p| p.starts_with("--config="))
        .expect("config file argument should be present");
    let config_path = config_path
        .split_once('=')
        .expect("config file argument should be correct")
        .1;

    let data = load_data(config_path);
    let tariffs = tariffs_to_15_minute_step(data.tariffs);
    let production = production_to_15_minute_step(data.production);
    let consumption = consumption_to_15_minute_step(data.consumption);

    let mut s = Schedule::new(None);
    //s.update_scheduling(&data.tariffs, &data.production, &data.consumption, 4.48, data.date_time);
    s.update_scheduling(&tariffs, &production, &consumption, 10, data.date_time);

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

fn tariffs_to_15_minute_step(data: Vec<TariffValues>) -> Vec<TariffValues> {
    let mut result: Vec<TariffValues> = Vec::new();

    for t in data {
        for q in 0..4 {
            result.push(TariffValues {
                valid_time: t.valid_time.add(TimeDelta::minutes(15 * q)),
                price: t.price,
                buy: t.buy,
                sell: t.sell,
            });
        }
    }

    result
}

fn production_to_15_minute_step(data: Vec<ProductionValues>) -> Vec<ProductionValues> {
    let mut result: Vec<ProductionValues> = Vec::new();

    for t in data {
        for q in 0..4 {
            result.push(ProductionValues {
                valid_time: t.valid_time.add(TimeDelta::minutes(15 * q)),
                power: t.power / 4.0,
            });
        }
    }

    result
}

fn consumption_to_15_minute_step(data: Vec<ConsumptionValues>) -> Vec<ConsumptionValues> {
    let mut result: Vec<ConsumptionValues> = Vec::new();

    for t in data {
        for q in 0..4 {
            result.push(ConsumptionValues {
                valid_time: t.valid_time.add(TimeDelta::minutes(15 * q)),
                power: t.power / 4.0,
            });
        }
    }

    result
}
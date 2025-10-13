use std::collections::HashMap;
use std::ops::Add;
use chrono::{DateTime, DurationRound, Local, TimeDelta, Timelike};
use crate::models::{ConsumptionValues, ProductionValues, TariffValues};

#[derive(Clone, Debug)]
struct Tariffs {
    buy: [f64;24],
    length: usize,
    offset: usize,
}

/// Available block types
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum BlockType {
    Charge,
    Hold,
    Use,
}

/// Block status
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Status {
    Waiting,
    Started,
    Missed,
    Full(usize),
    Error,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub block_type: BlockType,
    pub start_time: DateTime<Local>,
    pub end_time: DateTime<Local>,
    pub start_hour: usize,
    pub end_hour: usize,
    size: usize,
    pub cost: f64,
    pub charge_in: f64,
    pub charge_out: f64,
    pub soc_in: usize,
    pub soc_out: usize,
    pub status: Status,
}

#[derive(Clone, Debug)]
pub struct BlockInternal {
    pub block_type: BlockType,
    pub start_hour: usize,
    size: usize,
    pub cost: f64,
    pub charge_in: f64,
    pub charge_out: f64,
}

#[derive(Clone, Debug)]
struct Blocks {
    blocks: Vec<BlockInternal>,
    next_start: usize,
    next_charge_in: f64,
    total_cost: f64,
}

#[derive(Debug)]
struct PeriodMetrics {
    block_type: BlockType,
    start: usize,
    end: usize,
    charge_in: f64,
    charge_out: f64,
    hold_level: f64,
    cost: f64,
    discharged: Option<usize>,
}

/// Struct representing the block schedule from the current hour and forward
pub struct Schedule {
    pub date_time: DateTime<Local>,
    pub blocks: Vec<Block>,
    tariffs: Tariffs,
    pub total_cost: f64,
    net_prod: [f64;24],
    cons: [f64;24],
    bat_capacity: f64,
    bat_kwh: f64,
    soc_kwh: f64,
    charge_kwh_instance: f64,
    charge_efficiency: f64,
    discharge_efficiency: f64,
}

impl Schedule {
    /// Creates a new Schedule without scheduling
    ///
    /// # Arguments
    ///
    /// * 'config' - configuration struct
    pub fn new() -> Schedule {
        Schedule {
            date_time: Default::default(),
            blocks: Vec::new(),
            tariffs: Tariffs {
                buy: [0.0;24],
                length: 0,
                offset: 0,
            },
            total_cost: 0.0,
            net_prod: [0.0;24],
            cons: [0.0;24],
            bat_capacity: 16.59,
            bat_kwh: 14.931,
            soc_kwh: 0.1659,
            charge_kwh_instance: 6.0,
            charge_efficiency: 0.9,
            discharge_efficiency: 0.9,
        }
    }

    /// Updates scheduling based on tariffs, production and consumption estimates.
    /// It can also base the schedule on any residual charge and its mean charge tariff carrying in
    /// from a previous schedule or from the inverter itself (in which case it might be hard to determine
    /// the mean charge tariff).
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - tariffs as given from NordPool
    /// * 'production' - production estimates per hour
    /// * 'consumption' - consumption estimates per hour
    /// * 'charge_in' - any residual charge to bear in to the new schedule
    /// * 'charge_tariff_in' - the mean price for the residual price
    /// * 'date_time' - the date time to stamp on the schedule
    pub fn update_scheduling(&mut self, tariffs: &Vec<TariffValues>, production: &Vec<ProductionValues>, consumption: &Vec<ConsumptionValues>, charge_in: f64, date_time: DateTime<Local>) {
        let date_hour = date_time.duration_trunc(TimeDelta::hours(1)).unwrap();
        let tariffs_in_scope: Vec<(f64,f64)> = tariffs.iter()
            .filter(|t|t.valid_time >= date_hour && t.valid_time < date_hour.add(TimeDelta::days(1)))
            .map(|t|(t.buy, t.sell))
            .collect::<Vec<(f64,f64)>>();
        let allowed_length = tariffs_in_scope.len() as i64;

        let mut prod: [f64;24] = [0.0; 24];
        production.iter()
            .filter(|p|p.valid_time >= date_hour && p.valid_time < date_hour.add(TimeDelta::hours(allowed_length)))
            .enumerate()
            .for_each(|(i, p)| prod[i] = p.power / 1000.0);

        consumption.iter()
            .filter(|c|c.valid_time >= date_hour && c.valid_time < date_hour.add(TimeDelta::hours(allowed_length)))
            .enumerate()
            .for_each(|(i, p)| self.cons[i] = p.power / 1000.0);

        prod.iter()
            .enumerate()
            .for_each(|(i, &p)| self.net_prod[i] = p - self.cons[i]);

        self.date_time = date_time;
        self.tariffs = self.transform_tariffs(&tariffs_in_scope, date_hour.hour() as usize);
        let blocks = self.seek_best(charge_in);
        self.blocks = create_out_blocks(blocks.blocks, self.soc_kwh, date_time, date_hour.hour() as usize);

        //let block_vec = update_soc_and_end_values(blocks.blocks, self.soc_kwh);
        //self.blocks = adjust_for_offset(block_vec, date_time, date_hour.hour() as usize);
        self.total_cost = blocks.total_cost;
    }

    /// Seeks the best schedule given input parameters.
    /// The algorithm searches through all combinations of charge blocks, use blocks and charge levels
    /// and returns the one with the best price (i.e. the mean price for usage minus the price for charging).
    /// It also considers charge input from PV, which not only tops up batteries but also lowers the
    /// mean price for the stored energy, which in turn can be used for even lower hourly tariffs.
    ///
    /// # Arguments
    ///
    /// * 'charge_in' - any residual charge to bear in to the new schedule
    fn seek_best(&self, charge_in: f64) -> Blocks {
        let mut record: HashMap<usize, Blocks> = self.create_base_blocks(charge_in);

        for seek_first_charge in 0..self.tariffs.length {
            for charge_level_first in (0..=90).step_by(5) {

                println!("{} - {}", seek_first_charge, charge_level_first);

                let first_charge_blocks = self.seek_charge(0, seek_first_charge, charge_level_first, charge_in);

                for seek_first_use in first_charge_blocks.next_start..self.tariffs.length {
                    for use_end_first in seek_first_use..=self.tariffs.length {
                        if let Some(first_use_blocks) = self.seek_use(first_charge_blocks.next_start, seek_first_use, use_end_first, first_charge_blocks.next_charge_in) {
                            let first_combined = combine_blocks(&first_charge_blocks, &first_use_blocks);
                            self.record_best(1, &first_combined, &mut record);

                            for seek_second_charge in first_combined.next_start..self.tariffs.length {
                                for charge_level_second in (0..=90).step_by(5) {

                                    let second_charge_blocks = self.seek_charge(first_combined.next_start, seek_second_charge, charge_level_second, first_combined.next_charge_in);

                                    for seek_second_use in second_charge_blocks.next_start..self.tariffs.length {
                                        if let Some(second_use_blocks) = self.seek_use(second_charge_blocks.next_start, seek_second_use, self.tariffs.length, second_charge_blocks.next_charge_in) {
                                            let second_combined = combine_blocks(&second_charge_blocks, &second_use_blocks);
                                            let all_combined = combine_blocks(&first_combined, &second_combined);
                                            self.record_best(2, &all_combined, &mut record);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        get_best(&record)
    }

    /// Creates an initial base block as a backstop if the search doesn't find any charge/use
    /// opportunities.
    ///
    /// # Arguments
    ///
    /// * 'charge_in' - residual charge from the previous block
    fn create_base_blocks(&self, charge_in: f64) -> HashMap<usize, Blocks> {
        let mut record: HashMap<usize, Blocks> = HashMap::new();

        let pm = self.update_for_pv(BlockType::Use, 0, self.tariffs.length, charge_in);
        let block = self.get_none_charge_block(&pm);

        println!("{:?}", block);

        record.insert(0,Blocks {
            next_start: self.tariffs.length,
            next_charge_in: block.charge_out,
            total_cost: block.cost,
            blocks: vec![block],
        });

        record
    }

    /// Gets charge (and leading hold) block
    ///
    /// # Arguments
    ///
    /// * 'initial_start' - the initial start is used to calculate for a hold block prepending the charge block to bee
    /// * 'start' - start hour for the proposed charge block
    /// * 'soc_level' - the state of charge (SoC) to target the charge block for, it is given from 0-90 (10% is always reserved in the battery)
    /// * 'charge_in' - residual charge from previous block
    fn seek_charge(&self, initial_start: usize, start: usize, soc_level: usize, charge_in: f64) -> Blocks {
        let pm_hold = self.update_for_pv(BlockType::Hold, initial_start, start, charge_in);
        let hold = self.get_none_charge_block(&pm_hold);

        let need = (soc_level as f64 * self.soc_kwh - pm_hold.charge_out) / self.charge_efficiency;
        let charge: BlockInternal = if need > 0.0 {
            let (c_cost, end) = self.charge_cost_charge_end(start, need, None);
            let pm_charge = self.update_for_pv(BlockType::Charge, start, end, 0.0);

            self.get_charge_block(start, end - start, pm_hold.charge_out, soc_level as f64 * self.soc_kwh, c_cost + pm_charge.cost)

        } else {
            self.get_charge_block(start, 0, pm_hold.charge_out, pm_hold.charge_out, 0.0)
        };

        Blocks {
            next_start: charge.start_hour + charge.size,
            next_charge_in: charge.charge_out,
            total_cost: hold.cost + charge.cost,
            blocks: vec![hold, charge],
        }
    }

    /// Calculates the cost for a given charge from grid at a given start time
    /// It also returns the expected end for the charging period
    ///
    /// # Arguments
    ///
    /// * 'start' - start instance for charging from grid
    /// * 'charge' - charge in kWh
    /// * 'tariffs' - optional tariffs to use instead of the registered one in the Schedule struct
    fn charge_cost_charge_end(&self, start: usize, charge: f64, tariffs: Option<&Tariffs>) -> (f64, usize) {
        let mut instance_charge: Vec<f64> = Vec::new();
        let rem = charge % self.charge_kwh_instance;

        let use_tariffs = if let Some(tariffs) = tariffs {
            tariffs
        } else {
            &self.tariffs
        };

        (0..(charge / self.charge_kwh_instance) as usize).for_each(|_| instance_charge.push(self.charge_kwh_instance));
        if (rem * 10.0).round() as usize != 0 {
            instance_charge.push(rem);
        }
        let end = (start + instance_charge.len()).min(use_tariffs.length);
        let c_price = use_tariffs.buy[start..end].iter()
            .enumerate()
            .map(|(i, t)| instance_charge[i] * t)
            .sum::<f64>();

        (c_price, end)
    }

    /// Creates a charge block
    ///
    /// # Arguments
    ///
    /// * 'start' - the charge block starting hour
    /// * 'size' - length of charge block
    /// * 'charge_in' - residual charge from previous block
    /// * 'charge_out' - charge out after charging
    /// * 'cost' - the price, or cost, for charging
    fn get_charge_block(&self, start: usize, size: usize, charge_in: f64, charge_out: f64, cost: f64) -> BlockInternal {
        BlockInternal {
            block_type: BlockType::Charge,
            start_hour: start,
            size,
            cost,
            charge_in,
            charge_out,
        }
    }

    /// Seeks a use block
    ///
    /// # Arguments
    ///
    /// * 'initial_start' - the initial start is used to calculate for a hold block prepending the charge block to bee
    /// * 'seek_start' - where this run is supposed to start its search
    /// * 'seek_end' - where this run is supposed to end its search (non-inclusive)
    /// * 'charge_in' - residual charge from previous block
    fn seek_use(&self, initial_start: usize, seek_start: usize, seek_end: usize, charge_in: f64) -> Option<Blocks> {
        if seek_start == seek_end {
            return None;
        }

        let pm_hold = self.update_for_pv(BlockType::Hold, initial_start, seek_start, charge_in);
        let pm_use = self.update_for_pv(BlockType::Use, seek_start, seek_end, pm_hold.charge_out);

        let hold_block = self.get_none_charge_block(&pm_hold);
        let use_block = self.get_none_charge_block(&pm_use);

        Some(Blocks{
            next_start: use_block.start_hour + use_block.size,
            next_charge_in: use_block.charge_out,
            total_cost: hold_block.cost + use_block.cost,
            blocks: vec![hold_block, use_block],
        })

        /*
        let mut min_cost: Option<f64> = None;
        let mut best_pair: Option<(PeriodMetrics, PeriodMetrics)> = None;

        for u_start in seek_start..self.tariffs.length {

            let pm_hold = self.update_for_pv(BlockType::Hold, initial_start, u_start, charge_in, charge_tariff_in);
            let pm_use = self.update_for_pv(BlockType::Use, u_start, self.tariffs.length, pm_hold.charge_out, pm_hold.charge_tariff_out);

            let total_cost = pm_hold.cost + pm_use.cost - pm_hold.overflow_earn - pm_use.overflow_earn;

            if min_cost.is_none_or(|c| total_cost < c) {
                min_cost = Some(total_cost);
                best_pair = Some((pm_hold, pm_use));
            }
        }

        if let Some((pm_hold, pm_use)) = best_pair {
            let hold_block = self.get_none_charge_block(&pm_hold);
            let use_block = self.get_none_charge_block(&pm_use);

            Some(Blocks{
                schedule_id: None,
                next_start: use_block.start_hour + use_block.size,
                next_charge_in: use_block.charge_out,
                next_charge_tariff_in: use_block.charge_tariff_out,
                next_soc_in: use_block.soc_out,
                total_cost: hold_block.cost + use_block.cost - hold_block.overflow_earn - use_block.overflow_earn,
                blocks: vec![hold_block, use_block],
            })
        } else {
            None
        }
        */
    }

    /// Creates a hold or use block
    ///
    /// # Arguments
    ///
    /// * 'pm' - a PeriodMetrics struct
    fn get_none_charge_block(&self, pm: &PeriodMetrics) -> BlockInternal {
        BlockInternal {
            block_type: pm.block_type.clone(),
            start_hour: pm.start,
            size: pm.end - pm.start,
            cost: pm.cost,
            charge_in: pm.charge_in,
            charge_out: pm.charge_out,
        }
    }


    /// Updates stored charges and how addition from PV (free electricity) affects the mean price for the stored charge.
    /// Also, it breaks out any overflow, i.e. charge that exceeds the battery maximum, and the sell price for that overflow
    ///
    /// # Arguments
    ///
    /// * 'block_type' - The block type which is used to indicate how to deal with periods of net consumption
    /// * 'start' - block start hour
    /// * 'end' - block end hour (non-inclusive)
    /// * 'charge_in' - residual charge from previous block
    fn update_for_pv(&self, block_type: BlockType, start: usize, end: usize, charge_in: f64) -> PeriodMetrics {
        let mut pm = PeriodMetrics {
            block_type: block_type.clone(),
            start,
            end,
            charge_in,
            charge_out: charge_in,
            hold_level: if block_type != BlockType::Use {charge_in} else {0.0},
            cost: 0.0,
            discharged: if charge_in <= 0.0 {Some(start)} else {None},
        };

        if block_type == BlockType::Charge {
            pm.cost = self.cons[start..end].iter()
                .enumerate()
                .map(|(i, &c)| self.tariffs.buy[i+start] * c)
                .sum::<f64>();
        } else {
            self.net_prod[start..end].iter()
                .enumerate()
                .for_each(|(i, &np)| self.add_net_prod(i+start, np, &mut pm) );
        }

        pm
    }

    /// Adds net production for one instance of time and updates the given PeriodicMetrics
    /// accordingly.
    ///
    /// # Arguments
    ///
    /// * 'np_idx' - index of the time instance in the net production array
    /// * 'np_item' - net production for the time instance
    /// * 'pm' - the PeriodicMetrics to update
    fn add_net_prod(&self, np_idx: usize, np_item: f64, pm: &mut PeriodMetrics) {
        // If net production is negative, we will potentially draw power from the battery and thus
        // need to consider the efficiency of transforming battery stored energy into household energy
        let efficiency: f64 = if np_item < 0.0 {self.discharge_efficiency} else {1.0 / self.charge_efficiency};

        // net add is the currently expected charge out from the period with the addition of the
        // current time instance net production. The net production may be negative if the household
        // draws more power than the PV produces.
        let net_add = pm.charge_out + np_item / efficiency;
        if net_add < pm.hold_level {
            // If the net adding is negative, we need to buy energy from the grid and also revert
            // the efficiency previously added for drawing power from the battery.
            // Charge out from the time instance will be whatever hold level is set.
            pm.cost += self.tariffs.buy[np_idx] * (pm.hold_level - net_add) * efficiency;
            pm.charge_out = pm.hold_level;
        } else {
            // If the net adding is positive, we check whether the battery is full and thus will
            // sell power to the grid.
            // Charge out is set to eather max battery charge level or the net addition depending
            // on whether the battery is full or not.
            pm.charge_out = net_add.min(self.bat_kwh);
        }

        // The discharge indicator is set if the battery at any point is empty.
        if pm.discharged.is_none() && pm.charge_out <= 0.0 {
            pm.discharged = Some(np_idx);
        }
    }

    /// Prepares tariffs for offset management
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - hourly prices from NordPool (excl VAT)
    /// * 'offset' - the offset between first value in the arrays and actual start time
    fn transform_tariffs(&self, tariffs: &Vec<(f64, f64)>, offset: usize) -> Tariffs {
        let mut buy: [f64;24] = [0.0;24];
        tariffs.iter()
            .enumerate()
            .for_each(|(i, &t)| {
                buy[i] = t.0;
            });

        Tariffs { buy, length: tariffs.len(), offset }
    }

    /// Saves the given Blocks struct if the total cost is better than any stored for the level
    ///
    /// # Arguments
    ///
    /// * 'level' - level is 1 or 2 and indicates whether it is a first search result or a combined 2-step search
    /// * 'blocks' - the Blocks struct to check and potentially save as the best for its level
    /// * 'record' - the record of the best Blocks structs saved so far
    fn record_best(&self, level: usize, blocks: &Blocks, record: &mut HashMap<usize, Blocks>) {
        let contender = self.trim_and_tail(&blocks);

        if let Some(recorded_blocks) = record.get(&level) {
            if contender.total_cost < recorded_blocks.total_cost {
                record.insert(level, contender);
            }
        } else {
            record.insert(level, contender);
        }
    }

    /// Trims out any blocks with zero size (they are just artifacts from the search flow).
    /// Also, it makes sure that we fill any empty tail with a suitable hold block
    ///
    /// # Arguments
    ///
    /// * 'blocks' - the Blocks struct to trim and add tail to
    fn trim_and_tail(&self, blocks: &Blocks) -> Blocks {
        let mut result = blocks.clone();

        // Trim blocks with no length
        result.blocks = result.blocks.iter().filter(|b| b.size > 0).cloned().collect::<Vec<BlockInternal>>();

        if result.next_start < self.tariffs.length {
            let pm_hold = self.update_for_pv(BlockType::Hold, result.next_start, self.tariffs.length, result.next_charge_in);

            result.next_start = self.tariffs.length;
            result.next_charge_in = pm_hold.charge_out;
            result.total_cost += pm_hold.cost;

            result.blocks.push({
                self.get_none_charge_block(&pm_hold)
            });
        }
        result
    }
}

/// Creates output blocks by completing missing information and adding the offset
///
/// # Arguments
///
/// * 'blocks' - a vector of temporary internal blocks
/// * 'soc_kwh' - kWh per soc used to convert from charge to State of Charge
/// * 'date_time' - the date and time to be used to convert from hours to datetime in local TZ
/// * 'offset' - the offset to apply
fn create_out_blocks(blocks: Vec<BlockInternal>, soc_kwh: f64, date_time: DateTime<Local>, offset: usize) -> Vec<Block> {
    let mut result: Vec<Block> = Vec::new();
    let time = date_time.duration_trunc(TimeDelta::days(1)).unwrap();

    for b in blocks {
        let mut start_time = time;
        let mut end_time = time;

        let mut start_hour = b.start_hour + offset;
        if start_hour > 23 {
            start_hour -= 24;
            start_time = start_time.add(TimeDelta::days(1));
        }
        let mut end_hour = b.start_hour + b.size - 1 + offset;
        if end_hour > 23 {
            end_hour -= 24;
            end_time = end_time.add(TimeDelta::days(1));
        }

        result.push(Block {
            block_type: b.block_type.clone(),
            start_time: start_time.with_hour(start_hour as u32).unwrap(),
            end_time: end_time.with_hour(end_hour as u32).unwrap(),
            start_hour,
            end_hour,
            size: b.size,
            cost: b.cost,
            charge_in: b.charge_in,
            charge_out: b.charge_out,
            soc_in: 10 + (b.charge_in / soc_kwh).round().min(90.0) as usize,
            soc_out: 10 + (b.charge_out / soc_kwh).round().min(90.0) as usize,
            status: Status::Waiting,
        });
    }

    result
}

/// Combines two Blocks struct into one to get a complete day schedule
///
/// # Arguments
///
/// * 'blocks_one' - a blocks struct from a level one search
/// * 'blocks_two' - a blocks struct from a subsequent level two search
fn combine_blocks(blocks_one: &Blocks, blocks_two: &Blocks) -> Blocks {
    let mut combined = Blocks {
        blocks: blocks_one.blocks.clone(),
        next_start: blocks_two.next_start,
        next_charge_in: blocks_two.next_charge_in,
        total_cost: blocks_one.total_cost + blocks_two.total_cost,
    };
    combined.blocks.extend(blocks_two.blocks.clone());

    combined
}

/// Returns the best block from what has been recorded for various levels
///
/// # Arguments
///
/// * 'record' - the record of the best Blocks structs saved so far
fn get_best(record: &HashMap<usize, Blocks>) -> Blocks {
    let mut best_total: Option<f64> = None;
    let mut best_level: usize = 0;

    // Sometimes level 0, 1 and/or 2 have the same cost. To ensure the shortest schedule is chosen,
    // we need to ensure to start with the lowest level and going up.
    let mut keys: Vec<usize> = record.keys().cloned().collect();
    keys.sort();

    for l in keys {
        if best_total.is_none_or(|c| record[&l].total_cost < c) {
            best_total = Some(record[&l].total_cost);
            best_level = l;
        }
    }

    record[&best_level].clone()
}
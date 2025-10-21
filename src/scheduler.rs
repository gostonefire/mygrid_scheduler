use std::ops::Add;
use std::fmt;
use std::fmt::Formatter;
use chrono::{DateTime, DurationRound, Local, TimeDelta, Timelike, Utc};
use serde::{Deserialize, Serialize};
use crate::models::{ConsumptionValues, ProductionValues, TariffValues};

#[derive(Debug)]
pub struct Tariffs {
    pub buy: [f64;96],
    pub length: usize,
}

/// Available block types
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub enum BlockType {
    Charge,
    Hold,
    Use,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for BlockType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            BlockType::Charge => write!(f, "Charge"),
            BlockType::Hold   => write!(f, "Hold  "),
            BlockType::Use    => write!(f, "Use   "),
        }
    }
}

/// Block status
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub enum Status {
    Waiting,
    Started,
    Full(usize),
    Error,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for Status {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Status::Waiting => write!(f, "Waiting  "),
            Status::Started => write!(f, "Started  "),
            Status::Full(soc) => write!(f, "Full: {:>3}", soc),
            Status::Error   => write!(f, "Error    "),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Block {
    block_id: usize,
    pub block_type: BlockType,
    pub start_time: DateTime<Local>,
    pub end_time: DateTime<Local>,
    pub start_hour: usize,
    pub start_minute: usize,
    pub end_hour: usize,
    pub end_minute: usize,
    size: usize,
    pub cost: f64,
    pub charge_in: f64,
    pub charge_out: f64,
    pub soc_in: usize,
    pub soc_out: usize,
    soc_kwh: f64,
    pub status: Status,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for Block {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        // Build base output
        let output = format!("{} - {} -> {:>2} - {:>2}: SocIn {:>3}, SocOut {:>3}, chargeIn {:>5.2}, chargeOut {:>5.2}, cost {:>5.2} ",
                             self.status, self.block_type,
                             self.start_hour, self.end_hour,
                             self.soc_in, self.soc_out,
                             self.charge_in, self.charge_out,
                             self.cost);

        write!(f, "{}", output)
    }
}

impl Block {
    /// Updates the status of the block
    ///
    /// # Arguments
    ///
    /// * 'status' - the status to update with
    pub fn update_block_status(&mut self, status: Status) {
        if self.block_type == BlockType::Charge {
            if let Status::Full(soc) = status {
                self.soc_out = soc;
                self.charge_out = (soc - 10) as f64 * self.soc_kwh;
            }
        }
        self.status = status;
    }
}

#[derive(Clone)]
struct BlockInternal {
    block_type: BlockType,
    start_hour: usize,
    size: usize,
    cost: f64,
    charge_in: f64,
    charge_out: f64,
}

#[derive(Default)]
struct BlockCollection {
    blocks: Vec<BlockInternal>,
    next_start: usize,
    next_charge_in: f64,
    total_cost: f64,
}

struct PeriodMetrics {
    block_type: BlockType,
    start: usize,
    size: usize,
    charge_in: f64,
    charge_out: f64,
    hold_level: f64,
    cost: f64,
}

/// Struct representing the block schedule from the current hour and forward
pub struct Schedule {
    pub date_time: DateTime<Local>,
    pub blocks: Vec<Block>,
    pub tariffs: Tariffs,
    pub total_cost: f64,
    net_prod: [f64;96],
    cons: [f64;96],
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
    /// * 'schedule_blocks' - any existing schedule blocks
    pub fn new(schedule_blocks: Option<Vec<Block>>) -> Schedule {
        Schedule {
            date_time: Default::default(),
            blocks: schedule_blocks.unwrap_or(Vec::new()),
            tariffs: Tariffs {
                buy: [0.0; 96],
                length: 0,
            },
            total_cost: 0.0,
            net_prod: [0.0; 96],
            cons: [0.0; 96],
            bat_kwh: 14.931,
            soc_kwh: 0.1659,
            charge_kwh_instance: 6.0 / 4.0,
            charge_efficiency: 0.9,
            discharge_efficiency: 0.9,
        }
    }

    /// Returns a block id of a block identified by hour
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the time to get a block for
    pub fn get_block_by_time(&self, date_time: DateTime<Local>) -> Option<usize> {
        let date_hour = date_time.duration_trunc(TimeDelta::hours(1)).unwrap();
        for b in self.blocks.iter() {
            if b.start_time <= date_hour && b.end_time >= date_hour {
                return Some(b.block_id);
            }
        }

        None
    }

    /// Returns a mutable block identified by its block id
    ///
    /// # Arguments
    ///
    /// * 'block_ld' - id of the block
    pub fn get_block_by_id(&mut self, block_id: usize) -> Option<&mut Block> {
        self.blocks.iter_mut().find(|b| b.block_id == block_id)
    }

    /// Check if it is time to update to next step in schedule
    ///
    /// # Arguments
    ///
    /// * 'block_id' - id of the block to check
    /// * 'date_time' - the date time the block is valid for
    pub fn is_update_time(&self, block_id: usize, date_time: DateTime<Local>) -> bool {
        let date_hour = date_time.duration_trunc(TimeDelta::hours(1)).unwrap();
        let block = self.blocks.iter().find(|b| b.block_id == block_id);

        block.is_none_or(|b| (b.start_time > date_hour || b.end_time < date_hour) ||
            (b.start_time <= date_hour && b.end_time >= date_hour && b.status == Status::Waiting))
    }

    /// Check if we are in an active charge block and charging is still ongoing
    ///
    /// # Arguments
    ///
    /// * 'block_id' - id of the block to check
    /// * 'date_time' - the date time the block is valid for
    pub fn is_active_charging(&self, block_id: usize, date_time: DateTime<Local>) -> bool {
        let date_hour = date_time.duration_trunc(TimeDelta::hours(1)).unwrap();
        let block = self.blocks.iter().find(|b| b.block_id == block_id);

        block.is_some_and(|b| b.start_time <= date_hour && b.end_time >= date_hour
            && b.block_type == BlockType::Charge && b.status == Status::Started)
    }

    /// Updates scheduling based on tariffs, production and consumption estimates.
    /// It can also base the schedule on any residual charge (stated as soc).
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - tariffs as given from NordPool
    /// * 'production' - production estimates per hour
    /// * 'consumption' - consumption estimates per hour
    /// * 'soc_in' - any residual charge to bear in to the new schedule (stated as soc 0-100)
    /// * 'date_time' - the date time to stamp on the schedule
    pub fn update_scheduling(&mut self, tariffs: &Vec<TariffValues>, production: &Vec<ProductionValues>, consumption: &Vec<ConsumptionValues>, soc_in: u8, date_time: DateTime<Local>) {
        let date_hour = date_time.duration_trunc(TimeDelta::hours(1)).unwrap();
        let tariffs_in_scope: Vec<(f64, f64)> = tariffs.iter()
            .filter(|t| t.valid_time >= date_hour && t.valid_time < date_hour.add(TimeDelta::days(1)))
            .map(|t| (t.buy, t.sell))
            .collect::<Vec<(f64, f64)>>();
        let allowed_length = tariffs_in_scope.len() as i64;

        let mut prod: [f64; 96] = [0.0; 96];
        production.iter()
            .filter(|p| p.valid_time >= date_hour && p.valid_time < date_hour.add(TimeDelta::hours(allowed_length)))
            .enumerate()
            .for_each(|(i, p)| prod[i] = p.power / 1000.0);

        consumption.iter()
            .filter(|c| c.valid_time >= date_hour && c.valid_time < date_hour.add(TimeDelta::hours(allowed_length)))
            .enumerate()
            .for_each(|(i, p)| self.cons[i] = p.power / 1000.0);

        prod.iter()
            .enumerate()
            .for_each(|(i, &p)| self.net_prod[i] = p - self.cons[i]);

        let charge_in = (soc_in.max(10) - 10) as f64 * self.soc_kwh;

        self.date_time = date_time;
        self.tariffs = self.transform_tariffs(&tariffs_in_scope);
        let block_collection = self.seek_best(charge_in);
        self.blocks = create_result_blocks(block_collection.blocks, self.soc_kwh, date_time, date_hour.hour() as usize);
        self.total_cost = block_collection.total_cost;
    }

    /// Seeks the best schedule given input parameters.
    /// The algorithm searches through all combinations of charge blocks, use blocks and charge levels
    /// and returns the one with the lowest cost.
    ///
    /// It also considers charge input from PV.
    ///
    /// # Arguments
    ///
    /// * 'charge_in' - any residual charge to bear in to the new schedule
    fn seek_best(&self, charge_in: f64) -> BlockCollection {
        let mut quad: [BlockCollection; 4] = [Default::default(), Default::default(), Default::default(), Default::default()];

        let mut best_record: BlockCollection = self.create_base_block_collection(charge_in);

        for seek_first_charge in 0..self.tariffs.length {
            for charge_level_first in (0..=90).step_by(5) {
                println!("{:02} {:02}", seek_first_charge, charge_level_first);

                quad[0] = self.seek_charge(0, seek_first_charge, charge_level_first, charge_in);

                for seek_first_use in quad[0].next_start..self.tariffs.length {
                    for use_end_first in seek_first_use..=self.tariffs.length {
                        if let Some(first_use_collection) = self.seek_use(quad[0].next_start, seek_first_use, use_end_first, quad[0].next_charge_in) {
                            quad[1] = first_use_collection;

                            //if seek_first_charge == 17 && charge_level_first == 55 && seek_first_use == 32 && use_end_first == 96 {
                            //    println!();
                            //}

                            best_record = self.record_best_collection(&quad[0..2], best_record);

                            for seek_second_charge in quad[1].next_start..self.tariffs.length {
                                for charge_level_second in (0..=90).step_by(5) {
                                    quad[2] = self.seek_charge(quad[1].next_start, seek_second_charge, charge_level_second, quad[1].next_charge_in);

                                    for seek_second_use in quad[2].next_start..self.tariffs.length {
                                        if let Some(second_use_blocks) = self.seek_use(quad[2].next_start, seek_second_use, self.tariffs.length, quad[2].next_charge_in) {
                                            quad[3] = second_use_blocks;
                                            best_record = self.record_best_collection(&quad, best_record);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        best_record
    }

    /// Creates an initial base block collection as a backstop if the search doesn't find any charge/use
    /// opportunities.
    ///
    /// # Arguments
    ///
    /// * 'charge_in' - residual charge from the previous block
    fn create_base_block_collection(&self, charge_in: f64) -> BlockCollection {
        let pm = self.update_for_pv(BlockType::Use, 0, self.tariffs.length, charge_in);
        let block = self.get_none_charge_block(&pm);

        BlockCollection {
            next_start: self.tariffs.length,
            next_charge_in: block.charge_out,
            total_cost: (block.cost * 100.0).round() / 100.0,
            blocks: vec![block],
        }
    }

    /// Gets charge (and leading hold) block
    ///
    /// # Arguments
    ///
    /// * 'initial_start' - the initial start is used to calculate for a hold block prepending the charge block to bee
    /// * 'start' - start hour for the proposed charge block
    /// * 'soc_level' - the state of charge (SoC) to target the charge block for, it is given from 0-90 (10% is always reserved in the battery)
    /// * 'charge_in' - residual charge from previous block
    fn seek_charge(&self, initial_start: usize, start: usize, soc_level: usize, charge_in: f64) -> BlockCollection {
        let pm_hold = self.update_for_pv(BlockType::Hold, initial_start, start, charge_in);

        let mut blocks: Vec<BlockInternal> = Vec::with_capacity(2);
        if pm_hold.size > 0 {
            blocks.push(self.get_none_charge_block(&pm_hold));
        }

        let mut next_start = start;
        let mut next_charge_in = pm_hold.charge_out;
        let mut total_cost = pm_hold.cost;

        let need = (soc_level as f64 * self.soc_kwh - pm_hold.charge_out) / self.charge_efficiency;
        if need > 0.0 {
            let (c_cost, end) = self.charge_cost_charge_end(start, need, None);
            let pm_charge = self.update_for_pv(BlockType::Charge, start, end, 0.0);

            next_start += end - start;
            total_cost += c_cost + pm_charge.cost;

            if pm_charge.size > 0 {
                next_charge_in = soc_level as f64 * self.soc_kwh;
                blocks.push(self.get_charge_block(start, pm_charge.size, pm_hold.charge_out, next_charge_in, c_cost + pm_charge.cost));
            }
        }

        BlockCollection {
            next_start,
            next_charge_in,
            total_cost,
            blocks,
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
    fn seek_use(&self, initial_start: usize, seek_start: usize, seek_end: usize, charge_in: f64) -> Option<BlockCollection> {
        if seek_start == seek_end {
            return None;
        }

        let pm_hold = self.update_for_pv(BlockType::Hold, initial_start, seek_start, charge_in);
        let pm_use = self.update_for_pv(BlockType::Use, seek_start, seek_end, pm_hold.charge_out);

        let mut blocks: Vec<BlockInternal> = Vec::with_capacity(2);

        if pm_hold.size > 0 {
            blocks.push(self.get_none_charge_block(&pm_hold));
        }
        if pm_use.size > 0 {
            blocks.push(self.get_none_charge_block(&pm_use));
        }

        Some(BlockCollection {
            next_start: pm_use.start + pm_use.size,
            next_charge_in: pm_use.charge_out,
            total_cost: pm_hold.cost + pm_use.cost,
            blocks,
        })
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
            size: pm.size,
            cost: pm.cost,
            charge_in: pm.charge_in,
            charge_out: pm.charge_out,
        }
    }


    /// Updates stored charges and how addition from PV (free electricity) affects the stored charge.
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
            size: end - start,
            charge_in,
            charge_out: charge_in,
            hold_level: if block_type != BlockType::Use { charge_in } else { 0.0 },
            cost: 0.0,
        };

        if block_type == BlockType::Charge {
            pm.cost = self.cons[start..end].iter()
                .enumerate()
                .map(|(i, &c)| self.tariffs.buy[i + start] * c)
                .sum::<f64>();
        } else {
            self.net_prod[start..end].iter()
                .enumerate()
                .for_each(|(i, &np)| self.add_net_prod(i + start, np, &mut pm));
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
        let efficiency: f64 = if np_item < 0.0 { self.discharge_efficiency } else { 1.0 / self.charge_efficiency };

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
    }

    /// Prepares tariffs for offset management
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - hourly prices from NordPool (excl VAT)
    fn transform_tariffs(&self, tariffs: &Vec<(f64, f64)>) -> Tariffs {
        let mut buy: [f64; 96] = [0.0; 96];
        tariffs.iter()
            .enumerate()
            .for_each(|(i, &t)| {
                buy[i] = t.0;
            });

        Tariffs { buy, length: tariffs.len() }
    }

    /// Returns the best block collection compared between the latest results and the stored best
    ///
    /// # Arguments
    ///
    /// * 'quad' - the 2 or 4 blocks as stored in the quad variable
    /// * 'best_blocks' - the current best blocks recorded
    fn record_best_collection(&self, quad: &[BlockCollection], best_blocks: BlockCollection) -> BlockCollection {
        let quad_last = quad.len() - 1;
        let mut total_cost = quad.iter().map(|b| b.total_cost).sum::<f64>();
        let mut next_charge_in = quad[quad_last].next_charge_in;
        let mut pm: Option<PeriodMetrics> = None;
        let mut num_blocks: usize = 0;

        if quad[quad_last].next_start < self.tariffs.length {
            let pm_hold = self.update_for_pv(BlockType::Hold, quad[quad_last].next_start, self.tariffs.length, quad[quad_last].next_charge_in);
            total_cost += pm_hold.cost;
            next_charge_in = pm_hold.charge_out;
            pm = Some(pm_hold);
            num_blocks = 1;
        }

        total_cost = (total_cost * 100.0).round() / 100.0;

        if total_cost < best_blocks.total_cost {
            self.collect_blocks(quad, self.tariffs.length, next_charge_in, total_cost, pm)
        } else if total_cost == best_blocks.total_cost {
            num_blocks += quad.iter().map(|b| b.blocks.len()).sum::<usize>();
            if num_blocks < best_blocks.blocks.len() {
                self.collect_blocks(quad, self.tariffs.length, next_charge_in, total_cost, pm)
            } else {
                best_blocks
            }
        } else {
            best_blocks
        }
    }

    /// Collects blocks from the given quad array into one block collection structure
    ///
    /// # Arguments
    ///
    /// * 'quad' - the 2 or 4 blocks as stored in the quad variable
    /// * 'next_start' - to record
    /// * 'next_charge_in' - to record
    /// * 'total_cost' - to record
    /// * 'pm' - optional data for creation of an ending hold block
    fn collect_blocks(&self, quad: &[BlockCollection], next_start: usize, next_charge_in: f64, total_cost: f64, pm: Option<PeriodMetrics>) -> BlockCollection {
        let mut new_best_blocks = BlockCollection {
            next_start,
            next_charge_in,
            total_cost,
            blocks: quad.iter().map(|b| b.blocks.clone()).flatten().collect(),
        };
        if let Some(pm) = pm {
            new_best_blocks.blocks.push(self.get_none_charge_block(&pm));
        }

        new_best_blocks
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
fn create_result_blocks(blocks: Vec<BlockInternal>, soc_kwh: f64, date_time: DateTime<Local>, offset: usize) -> Vec<Block> {
    let mut result: Vec<Block> = Vec::new();
    let time = date_time.duration_trunc(TimeDelta::days(1)).unwrap();

    for b in blocks {
        let mut start_time = time;
        let mut end_time = time;

        let mut start_hour = b.start_hour / 4 + offset;
        let start_minute = b.start_hour % 4 * 15;
        if start_hour > 23 {
            start_hour -= 24;
            start_time = start_time.add(TimeDelta::days(1));
        }

        let mut end_hour = (b.start_hour + b.size - 1) / 4 + offset;
        let end_minute = (b.start_hour + b.size - 1) % 4 * 15;
        if end_hour > 23 {
            end_hour -= 24;
            end_time = end_time.add(TimeDelta::days(1));
        }

        start_time = start_time.with_hour(start_hour as u32).unwrap().with_minute(start_minute as u32).unwrap();
        end_time = end_time.with_hour(end_hour as u32).unwrap().with_minute(end_minute as u32).unwrap();

        result.push(Block {
            block_id: start_time.with_timezone(&Utc).timestamp() as usize,
            block_type: b.block_type.clone(),
            start_time,
            end_time,
            start_hour,
            start_minute,
            end_hour,
            end_minute,
            size: b.size,
            cost: b.cost,
            charge_in: b.charge_in,
            charge_out: b.charge_out,
            soc_in: 10 + (b.charge_in / soc_kwh).round().min(90.0) as usize,
            soc_out: 10 + (b.charge_out / soc_kwh).round().min(90.0) as usize,
            soc_kwh,
            status: Status::Waiting,
        });
    }

    result
}

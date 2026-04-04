use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MarketBid {
    pub node_id: String,
    pub fuel_multiplier: f32, // e.g., 0.8 for a 20% discount
    pub available_capacity_mb: u64,
    pub thermal_score: u8, // 0-100 (0 is cold, 100 is throttling)
    pub timestamp_us: u64,
}

#[derive(Debug)]
pub struct NodeVitals {
    pub cpu_idle_percent: AtomicU8,
    pub memory_free_mb: AtomicU64,
    pub thermal_pressure: AtomicU8,
}

impl Default for NodeVitals {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeVitals {
    pub fn new() -> Self {
        Self {
            cpu_idle_percent: AtomicU8::new(100),
            memory_free_mb: AtomicU64::new(1024),
            thermal_pressure: AtomicU8::new(40),
        }
    }
}

pub struct HiveMarket {
    pub active_bids: DashMap<String, MarketBid>,
    pub local_vitals: Arc<NodeVitals>,
    pub local_node_id: String,
}

impl HiveMarket {
    pub fn new(local_node_id: String) -> Self {
        Self {
            active_bids: DashMap::new(),
            local_vitals: Arc::new(NodeVitals::new()),
            local_node_id,
        }
    }

    pub fn calculate_local_bid(&self) -> MarketBid {
        let thermal = self.local_vitals.thermal_pressure.load(Ordering::Relaxed);
        let mem = self.local_vitals.memory_free_mb.load(Ordering::Relaxed);
        let cpu = self.local_vitals.cpu_idle_percent.load(Ordering::Relaxed);

        let fuel_multiplier = if thermal >= 95 {
            // Thermal Escape trigger
            5.0
        } else {
            // Simplified inverse score to fuel multiplier (lower = better)
            // Score node = (Memory_free * CPU_idle) / (T_pressure + 1)
            // Higher score = more capacity -> lower multiplier
            let score = ((mem as f32) * (cpu as f32)) / ((thermal as f32) + 1.0);

            // Baseline 1.0 at standard capacity (e.g. 1000MB, 100% CPU, 40C) -> score ~2439
            // A simple scale: Multiplier = 2500 / max(score, 1.0)
            let mut calculated = 2500.0 / score.max(1.0);
            calculated = calculated.clamp(0.5, 2.0);
            calculated
        };

        MarketBid {
            node_id: self.local_node_id.clone(),
            fuel_multiplier,
            available_capacity_mb: mem,
            thermal_score: thermal,
            timestamp_us: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
        }
    }

    pub fn process_bid(&self, bid: MarketBid) {
        self.active_bids.insert(bid.node_id.clone(), bid);
    }

    pub fn find_best_arbitrage(&self, current_node_id: &String) -> Option<MarketBid> {
        let current_bid = self.calculate_local_bid();
        let current_cost = current_bid.fuel_multiplier;

        let mut best_bid: Option<MarketBid> = None;
        let mut lowest_cost = current_cost;

        for entry in self.active_bids.iter() {
            let bid = entry.value();
            if bid.node_id == *current_node_id {
                continue;
            }

            // Apply 15% hysteresis buffer: New bid must be at least 15% cheaper
            // e.g. 0.85 * 1.0 = 0.85. If bid is 0.84, it wins.
            if bid.fuel_multiplier <= current_cost * 0.85
                && bid.fuel_multiplier < lowest_cost {
                    lowest_cost = bid.fuel_multiplier;
                    best_bid = Some(bid.clone());
                }
        }

        best_bid
    }
}

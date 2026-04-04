use std::sync::atomic::Ordering;
use tet_core::market::{HiveMarket, MarketBid};

#[test]
fn test_phase25_arbitrage_migration() {
    let market = HiveMarket::new("NodeA".to_string());

    // Simulate current node being expensive (multiplier = 1.5)
    // 2500 / 1.5 = 1666 max score -> reduce CPU and Memory
    market
        .local_vitals
        .cpu_idle_percent
        .store(50, Ordering::Relaxed);
    market
        .local_vitals
        .memory_free_mb
        .store(800, Ordering::Relaxed);
    market
        .local_vitals
        .thermal_pressure
        .store(60, Ordering::Relaxed);

    // Score ~ (800 * 50) / 61 = 655
    // Multiplier = 2500 / 655 = 3.8 (expensive, capped at 2.0)
    let local_bid = market.calculate_local_bid();
    assert_eq!(local_bid.fuel_multiplier, 2.0); // Capped at 2.0

    // Insert a much better neighbor (multiplier = 0.5)
    market.process_bid(MarketBid {
        node_id: "NodeB".to_string(),
        fuel_multiplier: 0.7,
        available_capacity_mb: 2000,
        thermal_score: 30,
        timestamp_us: 1000000,
    });

    let best_bid = market.find_best_arbitrage(&"NodeA".to_string());
    assert!(
        best_bid.is_some(),
        "Arbitrage migration should be triggered"
    );
    assert_eq!(best_bid.unwrap().node_id, "NodeB");
}

#[test]
fn test_phase25_thermal_escape() {
    let market = HiveMarket::new("NodeA".to_string());

    // Current node hits 96C
    market
        .local_vitals
        .thermal_pressure
        .store(96, Ordering::Relaxed);

    let local_bid = market.calculate_local_bid();
    assert_eq!(
        local_bid.fuel_multiplier, 5.0,
        "Thermal Escape multiplier must spike to 5.0"
    );

    // Neighbor is warm but below max
    market.process_bid(MarketBid {
        node_id: "NodeB".to_string(),
        fuel_multiplier: 1.5,
        available_capacity_mb: 2000,
        thermal_score: 80,
        timestamp_us: 1000000,
    });

    let best_bid = market.find_best_arbitrage(&"NodeA".to_string());
    assert!(best_bid.is_some());
    assert_eq!(best_bid.unwrap().node_id, "NodeB");
}

#[test]
fn test_phase25_equilibrium_jitter_prevention() {
    let market = HiveMarket::new("NodeA".to_string());

    // Set baseline so baseline bid = 1.0 (score = 2500)
    market
        .local_vitals
        .cpu_idle_percent
        .store(100, Ordering::Relaxed);
    market
        .local_vitals
        .memory_free_mb
        .store(1024, Ordering::Relaxed);
    market
        .local_vitals
        .thermal_pressure
        .store(40, Ordering::Relaxed);

    let local_bid = market.calculate_local_bid();
    let current_cost = local_bid.fuel_multiplier;
    assert!((current_cost - 1.0).abs() < 0.01);

    // Insert an identically priced node
    market.process_bid(MarketBid {
        node_id: "NodeB".to_string(),
        fuel_multiplier: 0.99,
        available_capacity_mb: 1024,
        thermal_score: 40,
        timestamp_us: 1000000,
    });

    // The neighbor is only ~1% cheaper. Must not trigger migration
    let best_bid = market.find_best_arbitrage(&"NodeA".to_string());
    assert!(
        best_bid.is_none(),
        "Jitter prevented: No migration for negligible gains"
    );

    // Now insert a bid at 0.84 (>15% cheaper)
    market.process_bid(MarketBid {
        node_id: "NodeC".to_string(),
        fuel_multiplier: 0.84, // 16% cheaper
        available_capacity_mb: 1024,
        thermal_score: 40,
        timestamp_us: 1000000,
    });

    let best_bid = market.find_best_arbitrage(&"NodeA".to_string());
    assert!(best_bid.is_some(), "Migration allowed at <0.85 threshold");
    assert_eq!(best_bid.unwrap().node_id, "NodeC");
}

use anyhow::Result;
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use crate::tet_cli::utils::{get_api_url, pb_style};

pub async fn hive_list(client: &Client) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message("Discovering Hive Peers...");

    let res = client
        .get(&format!("{}/v1/hive/peers", get_api_url()))
        .send()
        .await?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await?;
        let peers = body["peers"].as_array();
        match peers {
            Some(p) if !p.is_empty() => {
                pb.finish_with_message(format!(
                    "{} Found {} connected Hive Nodes:",
                    "✔".green(),
                    p.len()
                ));
                for peer in p {
                    println!(
                        "  {} [{}]",
                        peer["node_id"].as_str().unwrap_or("?").yellow(),
                        peer["public_addr"].as_str().unwrap_or("?").cyan()
                    );
                }
            }
            _ => {
                pb.finish_with_message(format!(
                    "{} No nodes in local P2P routing table.",
                    "ℹ".blue()
                ));
            }
        }
    } else {
        let raw = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        pb.finish_with_message(format!("{} Hive lookup Failed: {}", "✘".red(), raw));
    }

    Ok(())
}

pub async fn market_list(client: &Client) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message("Querying Global Market Rates...");

    let res = client
        .get(&format!("{}/v1/hive/peers", get_api_url()))
        .send()
        .await?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await?;
        let peers_val = body["peers"].as_array();
        match peers_val {
            Some(p) if !p.is_empty() => {
                let mut peers: Vec<_> = p.iter().collect();
                // Sort by price ascending
                peers.sort_by_key(|peer| {
                    peer["price_per_million_fuel"].as_u64().unwrap_or(u64::MAX)
                });

                pb.finish_with_message(format!(
                    "{} Live Market Active ({} providers discovered) 🌐",
                    "✔".green(),
                    p.len()
                ));
                println!(
                    "{0: <36} | {1: <15} | {2: <10} | {3: <10}",
                    "Node ID".bold(),
                    "Price (µFuel)".bold(),
                    "Capacity".bold(),
                    "Score".bold()
                );
                println!("{:-<77}", "");
                for peer in peers {
                    let id = peer["node_id"].as_str().unwrap_or("?");
                    let price = peer["price_per_million_fuel"].as_u64().unwrap_or(0);
                    let capacity = peer["available_capacity_mb"].as_u64().unwrap_or(0);
                    let rep = peer["min_reputation_score"].as_u64().unwrap_or(0);

                    let id_disp = if id.len() > 36 { &id[..36] } else { id };
                    println!(
                        "{0: <36} | {1: <15} | {2: <10} | {3: <10}",
                        id_disp.yellow(),
                        format!("{}", price).cyan(),
                        format!("{}MB", capacity),
                        rep
                    );
                }
            }
            _ => {
                pb.finish_with_message(format!("{} No Market Providers available.", "ℹ".blue()));
            }
        }
    } else {
        let raw = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        pb.finish_with_message(format!("{} Market query Failed: {}", "✘".red(), raw));
    }

    Ok(())
}

pub async fn swarm(client: &Client) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message("Compiling Swarm Topography...");

    let res = client
        .get(&format!("{}/v1/topology", get_api_url()))
        .send()
        .await?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await?;
        let edges = body.as_array();
        match edges {
            Some(e) if !e.is_empty() => {
                pb.finish_with_message(format!(
                    "{} Swarm Topology Active ({} edges)",
                    "✔".green(),
                    e.len()
                ));
                println!(
                    "{0: <15} -> {1: <15} | {2: <10} | {3: <12} | {4: <10} | {5: <10}",
                    "Source".bold(),
                    "Target".bold(),
                    "Calls".bold(),
                    "Errors".bold(),
                    "Latency (µs)".bold(),
                    "Bytes".bold()
                );
                println!("{:-<85}", "");

                let mut edges_vec: Vec<_> = e.iter().collect();
                // Sort by total bytes descending
                edges_vec.sort_by_key(|edge| {
                    std::cmp::Reverse(edge["total_bytes"].as_u64().unwrap_or(0))
                });

                for edge in edges_vec {
                    let source = edge["source"].as_str().unwrap_or("?");
                    let target = edge["target"].as_str().unwrap_or("?");
                    let calls = edge["call_count"].as_u64().unwrap_or(0);
                    let errors = edge["error_count"].as_u64().unwrap_or(0);
                    let latency = edge["total_latency_us"].as_u64().unwrap_or(0);
                    let bytes = edge["total_bytes"].as_u64().unwrap_or(0);

                    let avg_latency = if calls > 0 { latency / calls } else { 0 };
                    let err_fmt = if errors > 0 {
                        format!("{}", errors).red()
                    } else {
                        format!("{}", errors).green()
                    };

                    println!(
                        "{0: <15} -> {1: <15} | {2: <10} | {3: <12} | {4: <10} | {5: <10}",
                        source.yellow(),
                        target.cyan(),
                        calls,
                        err_fmt,
                        avg_latency,
                        bytes
                    );
                }
            }
            _ => {
                pb.finish_with_message(format!(
                    "{} No Swarm Telemetry recorded (Sandbox is quiet).",
                    "ℹ".blue()
                ));
            }
        }
    } else {
        let raw = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        pb.finish_with_message(format!(
            "{} Topology compilation Failed: {}",
            "✘".red(),
            raw
        ));
    }

    Ok(())
}

pub async fn ps_cmd(client: &Client, json_out: bool) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message("Scanning active agents across the Hive...");

    // Fetch metrics to get market vitals
    let metrics_res = client
        .get(&format!("{}/v1/swarm/metrics", get_api_url()))
        .send()
        .await;

    // Fetch topology for agent list
    let topo_res = client
        .get(&format!("{}/v1/topology", get_api_url()))
        .send()
        .await;

    // Fetch peers for market multipliers
    let peers_res = client
        .get(&format!("{}/v1/hive/peers", get_api_url()))
        .send()
        .await;

    let metrics: serde_json::Value = match metrics_res {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or(serde_json::json!({})),
        _ => serde_json::json!({}),
    };

    let topology: Vec<serde_json::Value> = match topo_res {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or_default(),
        _ => vec![],
    };

    let peers: serde_json::Value = match peers_res {
        Ok(r) if r.status().is_success() => r.json().await.unwrap_or(serde_json::json!({})),
        _ => serde_json::json!({}),
    };

    if json_out {
        let report = serde_json::json!({
            "agents": topology,
            "peers": peers["peers"],
            "metrics": metrics,
        });
        pb.finish_and_clear();
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    pb.finish_with_message(format!("{} Sovereign Hive Status", "✔".green()));

    // Agent topology table
    if !topology.is_empty() {
        println!(
            "\n{0: <15} → {1: <15} | {2: <8} | {3: <8} | {4: <12}",
            "Source".bold(),
            "Target".bold(),
            "Calls".bold(),
            "Errors".bold(),
            "Avg µs".bold()
        );
        println!("{:-<70}", "");
        for edge in &topology {
            let source = edge["source"].as_str().unwrap_or("?");
            let target = edge["target"].as_str().unwrap_or("?");
            let calls = edge["call_count"].as_u64().unwrap_or(0);
            let errors = edge["error_count"].as_u64().unwrap_or(0);
            let latency = edge["total_latency_us"].as_u64().unwrap_or(0);
            let avg = if calls > 0 { latency / calls } else { 0 };

            let err_col = if errors > 0 {
                format!("{}", errors).red()
            } else {
                format!("{}", errors).green()
            };

            println!(
                "{0: <15} → {1: <15} | {2: <8} | {3: <8} | {4: <12}",
                source.yellow(),
                target.cyan(),
                calls,
                err_col,
                avg
            );
        }
    } else {
        println!("\n  {} No active agents on this node.", "ℹ".blue());
    }

    // Market vitals
    println!(
        "\n{} Market Multiplier: {}x  |  Thermal: {}°C  |  Warp: {}µs  |  Oracle: {}µs",
        "📊".to_string(),
        metrics["fuel_efficiency_ratio"]
            .as_f64()
            .map(|v| format!("{:.2}", v))
            .unwrap_or("—".into())
            .cyan(),
        "—".yellow(),
        metrics["teleport_warp_us"]
            .as_u64()
            .map(|v| format!("{}", v))
            .unwrap_or("—".into()),
        metrics["oracle_verification_us"]
            .as_u64()
            .map(|v| format!("{}", v))
            .unwrap_or("—".into()),
    );

    Ok(())
}

pub async fn pay_cmd(client: &Client, from: &str, to: &str, amount: u64, json_out: bool) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "💰 Transferring {} fuel: {} → {}",
        amount.to_string().yellow(),
        from.cyan(),
        to.cyan()
    ));

    let payload = serde_json::json!({
        "source_alias": from,
        "target_alias": to,
        "amount": amount,
    });

    let res = client
        .post(&format!("{}/v1/tet/topup", get_api_url()))
        .json(&payload)
        .send()
        .await;

    match res {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or(serde_json::json!({}));
            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::to_string_pretty(&body)?);
            } else {
                pb.finish_with_message(format!(
                    "{} Transfer Complete: {} fuel ({} → {})",
                    "✔".green(),
                    amount.to_string().yellow(),
                    from.cyan(),
                    to.cyan()
                ));
            }
        }
        Ok(r) => {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            if json_out {
                pb.finish_and_clear();
                println!(
                    "{}",
                    serde_json::json!({"error": body, "status": status.as_u16()})
                );
            } else {
                pb.finish_with_message(format!(
                    "{} Transfer Failed (HTTP {}): {}",
                    "✘".red(),
                    status,
                    body
                ));
            }
        }
        Err(e) => {
            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::json!({"error": e.to_string()}));
            } else {
                pb.finish_with_message(format!("{} Network Error: {}", "✘".red(), e));
            }
        }
    }

    Ok(())
}

pub async fn logs_cmd(client: &Client, alias: Option<&str>, json_out: bool) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    let filter_msg = alias
        .map(|a| format!(" (filtering: {})", a.cyan()))
        .unwrap_or_default();
    pb.set_message(format!("Connecting to TelemetryHub...{}", filter_msg));

    // Connect to the WebSocket telemetry stream
    // In this implementation, we poll the metrics endpoint once and display
    // the current snapshot with human-readable icons. A persistent WebSocket
    // tail would use the /v1/swarm/stream endpoint.
    let res = client
        .get(&format!("{}/v1/swarm/metrics", get_api_url()))
        .send()
        .await;

    match res {
        Ok(r) if r.status().is_success() => {
            let body: serde_json::Value = r.json().await.unwrap_or(serde_json::json!({}));

            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::to_string_pretty(&body)?);
                return Ok(());
            }

            pb.finish_with_message(format!("{} TelemetryHub Live Snapshot", "✔".green()));

            println!(
                "  🧠 Oracle Fidelity:     {}µs (Ed25519 sign+verify per fetch)",
                body["oracle_verification_us"]
                    .as_u64()
                    .unwrap_or(0)
                    .to_string()
                    .cyan()
            );
            println!(
                "  ✈️  Teleport Warp:       {}µs (bincode serialize round-trip)",
                body["teleport_warp_us"]
                    .as_u64()
                    .unwrap_or(0)
                    .to_string()
                    .cyan()
            );
            println!(
                "  💰 Fuel Efficiency:      {}",
                body["fuel_efficiency_ratio"]
                    .as_f64()
                    .map(|v| format!("{:.4}", v))
                    .unwrap_or("—".into())
                    .yellow()
            );
            println!(
                "  🌡️  Market Evacuation:   {}ms (thermal panic drill)",
                body["market_evacuation_ms"]
                    .as_u64()
                    .unwrap_or(0)
                    .to_string()
                    .cyan()
            );
            println!(
                "  🧬 Mitosis Constant:     {}µs (CoW fork latency)",
                body["mitosis_latency_us"]
                    .as_u64()
                    .unwrap_or(0)
                    .to_string()
                    .cyan()
            );

            if let Some(a) = alias {
                println!("\n  {} Filtering for alias '{}' — connect to /v1/swarm/stream for live WebSocket tail.", "ℹ".blue(), a.yellow());
            }
        }
        Ok(r) => {
            let err = r.text().await.unwrap_or_default();
            pb.finish_with_message(format!("{} TelemetryHub unavailable: {}", "✘".red(), err));
        }
        Err(e) => {
            pb.finish_with_message(format!(
                "{} Cannot reach engine at {} — is it running? ({})",
                "✘".red(),
                get_api_url().yellow(),
                e
            ));
        }
    }

    Ok(())
}

pub async fn metrics_cmd(client: &Client, json_out: bool) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message("Running Northstar Benchmarking Suite...");

    let res = client
        .get(&format!("{}/v1/swarm/metrics", get_api_url()))
        .send()
        .await;

    match res {
        Ok(r) if r.status().is_success() => {
            let report: serde_json::Value = r.json().await.unwrap_or(serde_json::json!({}));

            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::to_string_pretty(&report)?);
                return Ok(());
            }

            pb.finish_with_message(format!("{} Northstar Report", "✔".green()));
            println!();
            println!(
                "  {0: <28} | {1: <15} | {2: <10}",
                "Metric".bold(),
                "Value".bold(),
                "Ceiling".bold()
            );
            println!("  {:-<60}", "");

            let warp = report["teleport_warp_us"].as_u64().unwrap_or(0);
            let warp_ok = if warp < 200_000 {
                "✔".green()
            } else {
                "✘".red()
            };
            println!(
                "  {0: <28} | {1: <15} | {2: <10} {3}",
                "Teleport Warp (µs)",
                warp.to_string().cyan(),
                "< 200,000",
                warp_ok
            );

            let mitosis = report["mitosis_latency_us"].as_u64().unwrap_or(0);
            let mitosis_ok = if mitosis < 15_000 {
                "✔".green()
            } else {
                "✘".red()
            };
            println!(
                "  {0: <28} | {1: <15} | {2: <10} {3}",
                "Mitosis Constant (µs)",
                mitosis.to_string().cyan(),
                "< 15,000",
                mitosis_ok
            );

            let oracle = report["oracle_verification_us"].as_u64().unwrap_or(0);
            let oracle_ok = if oracle < 5_000 {
                "✔".green()
            } else {
                "✘".red()
            };
            println!(
                "  {0: <28} | {1: <15} | {2: <10} {3}",
                "Oracle Fidelity (µs)",
                oracle.to_string().cyan(),
                "< 5,000",
                oracle_ok
            );

            let evac = report["market_evacuation_ms"].as_u64().unwrap_or(0);
            let evac_ok = if evac < 800 {
                "✔".green()
            } else {
                "✘".red()
            };
            println!(
                "  {0: <28} | {1: <15} | {2: <10} {3}",
                "Market Evacuation (ms)",
                evac.to_string().cyan(),
                "< 800",
                evac_ok
            );

            let eff = report["fuel_efficiency_ratio"].as_f64().unwrap_or(0.0);
            println!(
                "  {0: <28} | {1: <15} | {2: <10}",
                "Fuel Efficiency",
                format!("{:.4}", eff).yellow(),
                "higher=better"
            );
        }
        Ok(r) => {
            let err = r.text().await.unwrap_or_default();
            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::json!({"error": err}));
            } else {
                pb.finish_with_message(format!("{} Metrics unavailable: {}", "✘".red(), err));
            }
        }
        Err(e) => {
            if json_out {
                pb.finish_and_clear();
                println!("{}", serde_json::json!({"error": e.to_string()}));
            } else {
                pb.finish_with_message(format!(
                    "{} Cannot reach engine at {} — is it running? ({})",
                    "✘".red(),
                    get_api_url().yellow(),
                    e
                ));
            }
        }
    }

    Ok(())
}

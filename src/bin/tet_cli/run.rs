use anyhow::{anyhow, Result};
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use std::fs;
use tet_core::models::TetExecutionRequest;
use crate::tet_cli::utils::{get_api_url, pb_style};

pub async fn build_cmd(entry: &std::path::Path, out: &std::path::Path) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!("Compiling {} to Trytet Agent Component...", entry.display()));

    // 1. Verify the entry file exists
    if !entry.exists() {
        return Err(anyhow!("Entry file not found: {}", entry.display()));
    }

    // 2. Read the source
    let source = fs::read_to_string(entry)?;

    // 3. For the BYOL pipeline, we actually bundle the JS Evaluator Cartridge and inject the JS code.
    // In a full implementation, this uses `jco` to create a standalone component.
    // For this MVP, we will construct a valid .tet artifact that wraps the source.
    let base_dir = home::home_dir().unwrap_or_default().join(".trytet").join("cartridges");
    let local_path = std::env::current_dir().unwrap_or_default().join("crates").join("js-evaluator").join("target/wasm32-wasip1/release/js_evaluator.wasm");
    let global_path = base_dir.join("js_evaluator.wasm");
    
    let wasm_path = if local_path.exists() { local_path } else { global_path };
    
    let wasm_bytes = fs::read(&wasm_path).unwrap_or_else(|_| {
        println!("{} Warning: js_evaluator.wasm not found. Building empty agent shell.", "⚠".yellow());
        vec![]
    });

    let mut injected_files = std::collections::HashMap::new();
    injected_files.insert("agent.js".to_string(), source);

    let req = TetExecutionRequest {
        payload: Some(wasm_bytes),
        alias: Some("byol-agent".to_string()),
        env: std::collections::HashMap::new(),
        injected_files,
        allocated_fuel: 0, // Ignored in artifact build
        max_memory_mb: 64,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        manifest: None,
        egress_policy: None,
        target_function: None,
    };

    // Serialize to standard bincode representation
    use bincode::Options;
    let bincode_options = bincode::DefaultOptions::new()
        .with_fixint_encoding()
        .allow_trailing_bytes();
    
    let raw_bytes = bincode_options.serialize(&req)?;
    
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(out, raw_bytes)?;

    pb.finish_with_message(format!("{} Successfully compiled agent artifact to {}", "✔".green(), out.display()));
    Ok(())
}

pub async fn replay_cmd(client: &Client, snapshot_id: &str, payload: Option<&str>) -> Result<()> {
    if snapshot_id.contains('/') || snapshot_id.contains('\\') || snapshot_id.contains("..") {
        return Err(anyhow!("Invalid snapshot ID: contains path traversal characters"));
    }

    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!("Pulling snapshot {} from Hive...", snapshot_id.cyan()));

    let home_dir = home::home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;
    let path = home_dir.join(".trytet").join("snapshots").join(format!("{}.tet", snapshot_id));

    // If it doesn't exist locally, we normally hit a /v1/registry/pull or similar endpoint.
    // For the replay command, we actually hit the /v1/tet/export endpoint if local node has it.
    let export_url = format!("{}/v1/tet/export/{}", get_api_url(), snapshot_id);
    let export_res = client.get(&export_url).send().await?;

    if !export_res.status().is_success() {
        return Err(anyhow!("Failed to pull snapshot: {}", export_res.text().await?));
    }

    let bytes = export_res.bytes().await?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, &bytes)?;

    pb.finish_with_message(format!("{} Snapshot downloaded to {}", "✔".green(), path.display()));

    println!("\n{} Time-Travel Debugger Initialized", "►".magenta());
    println!("Resurrecting deterministic linear memory graph...");

    // Now we "up" the artifact, but we execute the payload provided to test the failure locally
    let mut req = TetExecutionRequest {
        payload: None,
        alias: Some(format!("replay-{}", snapshot_id)),
        env: std::collections::HashMap::new(),
        injected_files: std::collections::HashMap::new(),
        allocated_fuel: 50_000_000,
        max_memory_mb: 64,
        parent_snapshot_id: Some(snapshot_id.to_string()),
        call_depth: 0,
        voucher: None,
        manifest: None,
        egress_policy: None,
        target_function: None,
    };

    if let Some(p) = payload {
        req.payload = Some(p.as_bytes().to_vec());
    }

    let url = format!("{}/v1/tet/execute", get_api_url());
    let res = client.post(&url).json(&req).send().await?;

    if !res.status().is_success() {
        println!("{} Replay execution failed: {}", "✘".red(), res.text().await?);
        return Ok(());
    }

    let exec_res: tet_core::models::TetExecutionResult = res.json().await?;

    println!("\n=== REPLAY RESULTS ===");
    println!("Status: {:?}", exec_res.status);
    println!("Fuel Burned: {}", exec_res.fuel_consumed);
    println!("STDOUT:\n{}", exec_res.telemetry.stdout_lines.join("\n"));
    if !exec_res.telemetry.stderr_lines.is_empty() {
        println!("STDERR:\n{}", exec_res.telemetry.stderr_lines.join("\n"));
    }

    Ok(())
}

pub async fn run_payload(
    client: &Client,
    payload_path: &str,
    alias: &str,
    fuel: u64,
    memory: u64,
) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message("Preparing execution envelope...");

    let payload = fs::read(payload_path)?;
    let mut execution_payload = payload.clone();
    let mut injected_files = std::collections::HashMap::new();

    if payload_path.ends_with(".py") {
        pb.set_message("Base-Tet Python Interpreter detected. Injecting script...");
        // In a real environment, this pulls the WASI CPython Tet.
        // For testing, we mock it by treating the .py as the mock WASM and injecting the script as VFS.
        execution_payload = fs::read("tests/fixtures/python_mock.wasm").unwrap_or_else(|_| vec![]);
        let script_str = String::from_utf8_lossy(&payload).into_owned();
        injected_files.insert("script.py".to_string(), script_str);
    }

    pb.set_message(format!("Booting Tet: {} ...", alias.cyan()));

    let req = TetExecutionRequest {
        payload: Some(execution_payload),
        alias: Some(alias.to_string()),
        env: std::collections::HashMap::new(),
        injected_files,
        allocated_fuel: fuel,
        max_memory_mb: memory as u32,
        parent_snapshot_id: None,
        call_depth: 0,
        voucher: None,
        manifest: None,
        egress_policy: None,
        target_function: None,
    };

    let res = client
        .post(&format!("{}/v1/tet/execute", get_api_url()))
        .json(&req)
        .send()
        .await?;

    if res.status().is_success() {
        pb.finish_with_message(format!("{} Successfully Executed!", "✔".green()));
        let body: serde_json::Value = res.json().await?;
        println!("\nTet Output:\n{:#?}", body["telemetry"]);
    } else {
        pb.finish_with_message(format!("{} Execution Failed", "✘".red()));
        let code = res.status();
        let body = res.text().await?;
        println!("\nError (HTTP {}):\n{}", code, body);
    }

    Ok(())
}

pub async fn up_artifact(file: &std::path::PathBuf, fuel: Option<u64>) -> anyhow::Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "Resurrecting Sovereign Agent from '{}'...",
        file.display()
    ));

    let raw_bytes = std::fs::read(file)
        .map_err(|e| anyhow!("Failed to read artifact {}: {}", file.display(), e))?;

    let artifact = match tet_core::builder::TetBuilder::verify_and_load(&raw_bytes) {
        Ok(a) => a,
        Err(e) => {
            pb.finish_with_message(format!("{} Security Violation: {}", "✘".red(), e));
            std::process::exit(1);
        }
    };

    let node_workspace =
        std::env::current_dir()?.join(format!("agent_workspace_{}", uuid::Uuid::new_v4()));

    let ctx = tet_core::resurrection::ResurrectionContext {
        artifact,
        node_workspace,
    };

    pb.set_message("Booting Wasm Sandbox...");

    match ctx.boot(fuel).await {
        Ok(agent) => {
            if agent.result.status == tet_core::models::ExecutionStatus::Success {
                pb.finish_with_message(format!(
                    "{} Resurrection Complete! Agent exited cleanly.",
                    "✔".green()
                ));
            } else if agent.result.status == tet_core::models::ExecutionStatus::OutOfFuel {
                pb.finish_with_message(format!("{} Execution Trapped: OutOfFuel.", "✘".red()));
                std::process::exit(137);
            } else {
                pb.finish_with_message(format!(
                    "{} Execution Terminated: {:?}",
                    "✘".red(),
                    agent.result.status
                ));
                std::process::exit(1);
            }
        }
        Err(e) => {
            pb.finish_with_message(format!("{} Resurrection Failed: {}", "✘".red(), e));
            std::process::exit(1);
        }
    }

    Ok(())
}

use anyhow::{anyhow, Result};
use colored::*;
use indicatif::ProgressBar;
use reqwest::Client;
use std::fs;
use tet_core::models::TetExecutionRequest;
use crate::tet_cli::utils::{get_api_url, pb_style};

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

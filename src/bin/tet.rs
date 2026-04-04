#![allow(
    clippy::needless_borrows_for_generic_args,
    clippy::to_string_in_format_args
)]
use anyhow::{anyhow, Result};
use clap::{Parser, Subcommand};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tet_core::crypto::AgentWallet;
use tet_core::models::TetExecutionRequest;
use tet_core::models::{TetHashes, TetManifest};
use tet_core::sandbox::SnapshotPayload;

fn get_api_url() -> String {
    std::env::var("TRYTET_API_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

#[derive(Parser)]
#[command(name = "tet", about = "The Sovereign Hive Gateway", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Output raw JSON for piping into jq or automation scripts
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Resurrect and boot a .tet agent artifact natively in-process
    Up {
        /// Path to the .tet file
        file: std::path::PathBuf,
        /// Optional override for fuel limits
        #[arg(short, long)]
        fuel: Option<u64>,
    },
    /// Boot a Tet from a local payload or script
    Run {
        /// The Wasm module or script (.py) to execute
        payload_path: String,
        /// The assigned alias for the Tet in the mesh
        #[arg(long)]
        alias: String,
        /// Minimum injected fuel
        #[arg(long, default_value = "50000000")]
        fuel: u64,
        /// Max Memory MB bounds
        #[arg(long, default_value = "64")]
        memory: u64,
    },
    /// Captured atomic state of a hibernating Tet into a CAS .tet artifact
    Snapshot { alias: String, tag: String },
    /// OCI-compliant push of a .tet artifact to a remote registry
    Push {
        /// The tag to push (e.g. "my-registry.com/agent:v1")
        tag: String,
    },
    /// OCI-compliant pull of a .tet artifact from a remote registry
    Pull {
        /// The reference to pull (e.g. "my-registry.com/agent:v1")
        tag: String,
    },
    /// Authenticate with a remote OCI registry
    Login {
        /// Registry URL (e.g. "https://ghcr.io")
        registry: String,
        /// Authentication token
        #[arg(long)]
        token: String,
    },
    /// Triggers an immediate "teleportation" live migration of an active Tet to another node.
    Teleport { alias: String, target_node: String },
    /// Lists all known inter-connected Hive Nodes the Engine is federated with.
    HiveList,
    /// Discovers marketplace pricing and availability for node teleportation.
    MarketList,
    /// Visualizes the LIVE Swarm Telemetry matrix native to this Sandbox.
    Swarm,
    /// Bridges an internal Tet Alias to the Legacy Internet via a public Ingress Route
    Bridge {
        /// The target Tet alias (e.g., my-agent)
        alias: String,
        /// The public URL path to expose (e.g., /ingress/my-agent)
        #[arg(long)]
        path: String,
    },
    /// Query the Sovereign Memory of a particular Tet
    Memory {
        alias: String,
        #[arg(long)]
        vector: String,
    },
    /// Perform neural inference on a Tet's loaded model
    Infer {
        /// The target Tet alias
        alias: String,
        /// The prompt to send to the model
        prompt: String,
        /// The model alias to use (e.g., llama-3-8b)
        #[arg(long, default_value = "default")]
        model: String,
        /// Sampling temperature
        #[arg(long, default_value = "0.7")]
        temperature: f32,
        /// Maximum tokens to generate
        #[arg(long, default_value = "256")]
        max_tokens: u32,
    },
    /// List active agents and their operational vitals (Market Multiplier, Thermal Pressure)
    Ps,
    /// Transfer fuel credits between agents
    Pay {
        /// Source agent alias or pubkey
        from: String,
        /// Destination agent alias or pubkey
        to: String,
        /// Amount of fuel to transfer
        amount: u64,
    },
    /// Tail the TelemetryHub with human-readable event icons
    Logs {
        /// Agent alias to follow
        #[arg(short = 'f', long = "follow")]
        alias: Option<String>,
    },
    /// Run the Northstar Benchmarking Suite and display performance metrics
    Metrics,
}

fn pb_style() -> ProgressStyle {
    ProgressStyle::with_template("{spinner:.cyan} {msg} [{elapsed_precise}]")
        .unwrap()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ ")
}

async fn run_payload(
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

async fn snapshot(client: &Client, alias: &str, tag: &str) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!("Snapshotting {} natively...", alias.cyan()));

    // 1. Tell engine to snapshot the alias
    let res = client
        .post(&format!("{}/v1/tet/snapshot/{}", get_api_url(), alias))
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(anyhow!("Failed engine snapshot"));
    }
    let body: serde_json::Value = res.json().await?;
    let snapshot_id = body["snapshot_id"].as_str().unwrap().to_string();

    pb.set_message("Exporting atomic RAM and VFS payload...");

    // 2. Export raw SnapshotPayload
    let payload_res = client
        .get(&format!("{}/v1/tet/export/{}", get_api_url(), snapshot_id))
        .send()
        .await?;
    if !payload_res.status().is_success() {
        return Err(anyhow!("Failed to export snapshot payload"));
    }
    let payload: SnapshotPayload = payload_res.json().await?;

    pb.set_message("Hashing artifacts & Signing manifest...");

    // 3. Hash components
    let wasm_hash = hex::encode(Sha256::digest(&payload.wasm_bytes));
    let memory_hash = hex::encode(Sha256::digest(&payload.memory_bytes));
    let vfs_hash = hex::encode(Sha256::digest(&payload.fs_tarball));

    // 4. Create Manifest
    let wallet = AgentWallet::load_or_create()?;
    let hashes = TetHashes {
        wasm_hash,
        memory_hash,
        vfs_hash,
    };

    let manifest = TetManifest {
        name: tag.to_string(),
        version: "1.0".into(),
        created_at: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
        author_pubkey: wallet.public_key_hex(),
        hashes,
    };

    // 5. Build `.tet` local temporary representation (tar)
    let home_dir = home::home_dir().unwrap();
    let local_builds = home_dir.join(".trytet").join("builds");
    fs::create_dir_all(&local_builds)?;
    let tet_file = local_builds.join(format!("{}.tet", tag.replace("/", "_")));

    let file = fs::File::create(&tet_file)?;
    let mut builder = tar::Builder::new(file);

    let manifest_bytes = serde_json::to_vec(&manifest)?;
    let mut h1 = tar::Header::new_gnu();
    h1.set_size(manifest_bytes.len() as u64);
    h1.set_cksum();
    builder.append_data(&mut h1, "manifest.json", manifest_bytes.as_slice())?;

    let mut h2 = tar::Header::new_gnu();
    h2.set_size(payload.wasm_bytes.len() as u64);
    h2.set_cksum();
    builder.append_data(&mut h2, "module.wasm", payload.wasm_bytes.as_slice())?;

    let mut h3 = tar::Header::new_gnu();
    h3.set_size(payload.memory_bytes.len() as u64);
    h3.set_cksum();
    builder.append_data(&mut h3, "memory.bin", payload.memory_bytes.as_slice())?;

    let mut h4 = tar::Header::new_gnu();
    h4.set_size(payload.fs_tarball.len() as u64);
    h4.set_cksum();
    builder.append_data(&mut h4, "vfs.tar", payload.fs_tarball.as_slice())?;

    let mut h5 = tar::Header::new_gnu();
    h5.set_size(payload.vector_idx.len() as u64);
    h5.set_cksum();
    builder.append_data(&mut h5, "vector.idx", payload.vector_idx.as_slice())?;

    let mut h6 = tar::Header::new_gnu();
    h6.set_size(payload.inference_state.len() as u64);
    h6.set_cksum();
    builder.append_data(&mut h6, "inference.bin", payload.inference_state.as_slice())?;

    builder.finish()?;

    pb.finish_with_message(format!(
        "{} Captured atomic snapshot to {}",
        "✔".green(),
        tag.cyan()
    ));

    Ok(())
}

async fn push(tag: &str) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!("Authenticating and Pushing {}...", tag.cyan()));

    let auth = load_auth()?;
    let registry_url = if tag.contains('/') {
        let parts: Vec<&str> = tag.split('/').collect();
        format!("https://{}", parts[0])
    } else {
        "https://index.docker.io".to_string()
    };

    let token = auth.get(&registry_url).cloned();
    let client = tet_core::registry::oci::OciClient::new(registry_url, token);

    let home_dir = home::home_dir().unwrap();
    let tet_file = home_dir
        .join(".trytet")
        .join("builds")
        .join(format!("{}.tet", tag.replace("/", "_").replace(":", "_")));

    if !tet_file.exists() {
        return Err(anyhow!("No local snapshot build found for tag {}", tag));
    }

    let bytes = fs::read(&tet_file)?;
    let artifact = tet_core::builder::TetBuilder::verify_and_load(&bytes)?;

    client.push(&artifact, tag).await?;

    pb.finish_with_message(format!(
        "{} Published to Registry via OCI Distribution!",
        "✔".green()
    ));
    Ok(())
}

fn load_auth() -> Result<std::collections::HashMap<String, String>> {
    let path = home::home_dir().unwrap().join(".trytet").join("auth.json");
    if path.exists() {
        let data = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&data)?)
    } else {
        Ok(std::collections::HashMap::new())
    }
}

async fn login(registry: &str, token: &str) -> Result<()> {
    let mut auth = load_auth()?;
    auth.insert(registry.to_string(), token.to_string());

    let path = home::home_dir().unwrap().join(".trytet").join("auth.json");
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, serde_json::to_string(&auth)?)?;

    println!(
        "{} Scoped authentication token persisted for {}",
        "✔".green(),
        registry.cyan()
    );
    Ok(())
}

async fn pull(tag: &str) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!("Pulling {} from Registry...", tag.cyan()));

    let auth = load_auth()?;
    let registry_url = if tag.contains('/') {
        let parts: Vec<&str> = tag.split('/').collect();
        format!("https://{}", parts[0])
    } else {
        "https://index.docker.io".to_string()
    };

    let token = auth.get(&registry_url).cloned();
    let client = tet_core::registry::oci::OciClient::new(registry_url, token);
    let cache = tet_core::registry::cache::ArtifactCache::new()?;

    let artifact = client.pull(tag).await?;

    // Store in CA cache
    let wasm_digest = format!(
        "sha256:{}",
        hex::encode(sha2::Sha256::digest(&artifact.blueprint_wasm))
    );
    cache.store_blob(&wasm_digest, &artifact.blueprint_wasm)?;

    pb.finish_with_message(format!(
        "{} Pulled artifact to local CAS cache. Ready for resurrection.",
        "✔".green()
    ));

    Ok(())
}

async fn teleport(client: &Client, alias: &str, target_node: &str) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "Initiating Live Migration for {} to {}...",
        alias.cyan(),
        target_node.yellow()
    ));

    let res = client
        .post(&format!("{}/v1/tet/teleport/{}", get_api_url(), alias))
        .json(&serde_json::json!({ "target_node": target_node }))
        .send()
        .await?;

    if res.status().is_success() {
        let body: serde_json::Value = res.json().await?;
        let msg = body["message"].as_str().unwrap_or("Teleport successful");
        pb.finish_with_message(format!("{} {}", "✔".green(), msg));
    } else {
        let raw = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        pb.finish_with_message(format!("{} Teleport Failed: {}", "✘".red(), raw));
    }

    Ok(())
}

async fn hive_list(client: &Client) -> Result<()> {
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

async fn market_list(client: &Client) -> Result<()> {
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

async fn swarm(client: &Client) -> Result<()> {
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::new();

    match &cli.command {
        Commands::Up { file, fuel } => up_artifact(file, *fuel).await,
        Commands::Run {
            payload_path,
            alias,
            fuel,
            memory,
        } => run_payload(&client, payload_path, alias, *fuel, *memory).await,
        Commands::Snapshot { alias, tag } => snapshot(&client, alias, tag).await,
        Commands::Push { tag } => push(tag).await,
        Commands::Pull { tag } => pull(tag).await,
        Commands::Login { registry, token } => login(registry, token).await,
        Commands::Teleport { alias, target_node } => teleport(&client, alias, target_node).await,
        Commands::HiveList => hive_list(&client).await,
        Commands::MarketList => market_list(&client).await,
        Commands::Swarm => swarm(&client).await,
        Commands::Bridge { alias, path } => bridge(&client, alias, path).await,
        Commands::Memory { alias, vector } => memory_query(&client, alias, vector).await,
        Commands::Infer {
            alias,
            prompt,
            model,
            temperature,
            max_tokens,
        } => infer_cmd(&client, alias, prompt, model, *temperature, *max_tokens).await,
        Commands::Ps => ps_cmd(&client, cli.json).await,
        Commands::Pay { from, to, amount } => pay_cmd(&client, from, to, *amount, cli.json).await,
        Commands::Logs { alias } => logs_cmd(&client, alias.as_deref(), cli.json).await,
        Commands::Metrics => metrics_cmd(&client, cli.json).await,
    }
}

async fn bridge(client: &Client, alias: &str, path: &str) -> Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "Bridging alias {} -> {}",
        alias.cyan(),
        path.green()
    ));

    let route = tet_core::oracle::IngressRoute {
        public_path: path.to_string(),
        target_alias: alias.to_string(),
        method_filter: vec![], // Allow all
    };

    let res = client
        .post(&format!("{}/v1/ingress/register", get_api_url()))
        .json(&route)
        .send()
        .await?;

    if res.status().is_success() {
        pb.finish_with_message(format!(
            "{} Successfully mapped Gateway: {} ↔ Mesh: {}",
            "✔".green(),
            path.yellow(),
            alias.cyan()
        ));
    } else {
        let raw = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        pb.finish_with_message(format!("{} Bridge mapping failed: {}", "✘".red(), raw));
    }

    Ok(())
}

async fn memory_query(client: &reqwest::Client, alias: &str, vector: &str) -> anyhow::Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!("Querying semantic memory for {}...", alias.cyan()));

    // Parse the float string (e.g. "[0.1, 0.2]")
    let vec_query: Result<Vec<f32>, _> = serde_json::from_str(vector);
    let vector_data = match vec_query {
        Ok(v) => v,
        Err(_) => {
            pb.finish_with_message(format!(
                "{} Vector must be floating array like [0.1, 0.2]",
                "✘".red()
            ));
            return Ok(());
        }
    };

    let query_payload = tet_core::memory::SearchQuery {
        collection: "default".to_string(), // Default space
        query_vector: vector_data,
        limit: 5,
        min_score: 0.0,
    };

    let res = client
        .post(&format!("{}/v1/tet/memory/{}", get_api_url(), alias))
        .json(&query_payload)
        .send()
        .await?;

    if res.status().is_success() {
        let results: Vec<tet_core::memory::SearchResult> = res.json().await?;
        pb.finish_with_message(format!(
            "{} Retrieved {} Semantic Matches from {}",
            "✔".green(),
            results.len(),
            alias.cyan()
        ));
        for (i, result) in results.iter().enumerate() {
            println!(
                "  [Match {}] ID: {} (Score: {:.4})",
                i,
                result.id.yellow(),
                result.score
            );
        }
    } else {
        let err = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown API Error".into());
        pb.finish_with_message(format!("{} Memory search failed: {}", "✘".red(), err));
    }

    Ok(())
}

async fn infer_cmd(
    client: &reqwest::Client,
    alias: &str,
    prompt: &str,
    model: &str,
    temperature: f32,
    max_tokens: u32,
) -> anyhow::Result<()> {
    let pb = ProgressBar::new_spinner();
    pb.set_style(pb_style());
    pb.set_message(format!(
        "Invoking Sovereign Inference on {}...",
        alias.cyan()
    ));

    let request = tet_core::inference::InferenceRequest {
        model_alias: model.to_string(),
        prompt: prompt.to_string(),
        temperature,
        max_tokens,
        stop_sequences: Vec::new(),
        session_id: None,
        deterministic_seed: 42,
    };

    let res = client
        .post(&format!("{}/v1/tet/infer/{}", get_api_url(), alias))
        .json(&request)
        .send()
        .await?;

    if res.status().is_success() {
        let response: tet_core::inference::InferenceResponse = res.json().await?;
        pb.finish_with_message(format!(
            "{} Inference Complete ({})",
            "✔".green(),
            response.model_alias.cyan()
        ));
        println!("\n{}", response.text);
        println!(
            "\n  {} Prompt Tokens: {}, Generated: {}, Fuel Burned: {}",
            "⚡".yellow(),
            response.prompt_tokens,
            response.tokens_generated,
            response.fuel_burned
        );
        println!("  {} Session: {}", "🧠".to_string(), response.session_id);
    } else {
        let err = res
            .text()
            .await
            .unwrap_or_else(|_| "Unknown API Error".into());
        pb.finish_with_message(format!("{} Inference failed: {}", "✘".red(), err));
    }

    Ok(())
}

async fn up_artifact(file: &std::path::PathBuf, fuel: Option<u64>) -> anyhow::Result<()> {
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

// ---------------------------------------------------------------------------
// Phase 27.1: Control Plane — New Handlers
// ---------------------------------------------------------------------------

async fn ps_cmd(client: &Client, json_out: bool) -> Result<()> {
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

async fn pay_cmd(client: &Client, from: &str, to: &str, amount: u64, json_out: bool) -> Result<()> {
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

async fn logs_cmd(client: &Client, alias: Option<&str>, json_out: bool) -> Result<()> {
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

async fn metrics_cmd(client: &Client, json_out: bool) -> Result<()> {
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

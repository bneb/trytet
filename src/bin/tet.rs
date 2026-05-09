#![allow(
    clippy::needless_borrows_for_generic_args,
    clippy::to_string_in_format_args
)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use reqwest::Client;

mod tet_cli;

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
    /// Register a local artifact as a global Sovereign Agent
    Push {
        path: std::path::PathBuf,
        alias: String,
    },
    /// Pull an agent and its genesis state from the Hive
    Pull {
        alias: String,
        #[arg(short, long)]
        version: Option<String>,
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
    /// Start the Trytet MCP (Model Context Protocol) Server over stdio
    Mcp,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::new();

    match &cli.command {
        Commands::Up { file, fuel } => tet_cli::run::up_artifact(file, *fuel).await,
        Commands::Run {
            payload_path,
            alias,
            fuel,
            memory,
        } => tet_cli::run::run_payload(&client, payload_path, alias, *fuel, *memory).await,
        Commands::Snapshot { alias, tag } => tet_cli::snapshot::snapshot(&client, alias, tag).await,
        Commands::Push { path, alias } => tet_cli::registry::push(path, alias).await,
        Commands::Pull { alias, version } => tet_cli::registry::pull(alias, version.as_deref()).await,
        Commands::Login { registry, token } => tet_cli::registry::login(registry, token).await,
        Commands::Teleport { alias, target_node } => tet_cli::network::teleport(&client, alias, target_node).await,
        Commands::HiveList => tet_cli::status::hive_list(&client).await,
        Commands::MarketList => tet_cli::status::market_list(&client).await,
        Commands::Swarm => tet_cli::status::swarm(&client).await,
        Commands::Bridge { alias, path } => tet_cli::network::bridge(&client, alias, path).await,
        Commands::Memory { alias, vector } => tet_cli::memory::memory_query(&client, alias, vector).await,
        Commands::Infer {
            alias,
            prompt,
            model,
            temperature,
            max_tokens,
        } => tet_cli::infer::infer_cmd(&client, alias, prompt, model, *temperature, *max_tokens).await,
        Commands::Ps => tet_cli::status::ps_cmd(&client, cli.json).await,
        Commands::Pay { from, to, amount } => tet_cli::status::pay_cmd(&client, from, to, *amount, cli.json).await,
        Commands::Logs { alias } => tet_cli::status::logs_cmd(&client, alias.as_deref(), cli.json).await,
        Commands::Metrics => tet_cli::status::metrics_cmd(&client, cli.json).await,
        Commands::Mcp => tet_cli::mcp::mcp_cmd().await,
    }
}

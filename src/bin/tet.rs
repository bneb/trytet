#![allow(
    clippy::needless_borrows_for_generic_args,
    clippy::to_string_in_format_args
)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use reqwest::Client;

mod tet_cli;

#[derive(Parser)]
#[command(name = "tet", about = "Trytet sandbox engine CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    /// Output raw JSON for piping into jq or automation scripts
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the API server, or boot a .tet agent artifact when given a file
    Up {
        /// Path to the .tet file (if omitted, starts the API server)
        file: Option<std::path::PathBuf>,
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
    /// Push an artifact to the registry
    Push {
        path: std::path::PathBuf,
        alias: String,
    },
    /// List active agents and their operational vitals (Market Multiplier, Thermal Pressure)
    Ps,
    /// Tail the TelemetryHub with human-readable event icons
    Logs {
        /// Agent alias to follow
        #[arg(short = 'f', long = "follow")]
        alias: Option<String>,
    },
    /// Run the Northstar Benchmarking Suite and display performance metrics
    Metrics,
    /// Execute code in a fuel-metered sandbox (like `node -e` but sandboxed)
    Exec {
        /// JavaScript or Python code to execute
        code: String,
        /// Language: js or py (default: js)
        #[arg(short, long, default_value = "js")]
        lang: String,
        /// Fuel budget (default: 5M instructions)
        #[arg(short, long, default_value = "5000000")]
        fuel: u64,
    },
    /// Start the Trytet MCP (Model Context Protocol) Server over stdio
    Mcp {
        /// List registered tools and exit (don't start the server)
        #[arg(long)]
        list_tools: bool,
    },
    /// Start the API server on 0.0.0.0:3000
    Serve,
    /// Time-Travel Replay Debugger: Download a crashed state snapshot and replay deterministically
    Replay {
        /// The snapshot ID to pull and replay
        snapshot_id: String,
        /// The payload to evaluate upon resuming the state
        #[arg(short, long)]
        payload: Option<String>,
    },
    /// Bring Your Own Language (BYOL) Build Pipeline: Compile TS/JS to a Trytet Agent
    Build {
        /// Path to the TypeScript or JavaScript entry point
        entry: std::path::PathBuf,
        /// Output .tet Wasm artifact path
        #[arg(short, long)]
        out: std::path::PathBuf,
    },
    /// Cartridge marketplace: publish a cartridge to the registry
    Publish {
        /// Path to the .wasm cartridge file
        path: std::path::PathBuf,
        /// Tag for this cartridge version
        #[arg(short, long)]
        tag: String,
    },
    /// Cartridge marketplace: search the registry
    Search {
        /// Search query
        query: String,
    },
    /// Validate a cartridge for WIT conformance
    Validate {
        /// Path to the .wasm cartridge file
        path: std::path::PathBuf,
    },
    /// Create an API key
    KeyCreate {
        /// Label for this key
        #[arg(short, long)]
        label: String,
    },
    /// List active API keys
    KeyList,
    /// Revoke an API key by prefix
    KeyRevoke {
        /// Key prefix to revoke
        prefix: String,
    },
    /// Initialize a new Trytet agent project
    Init {
        /// Project name
        name: Option<String>,
    },
    /// Diagnose install health: cartridge paths, engine status, config
    Doctor,
    /// Register Trytet as an MCP server with Claude Desktop, Cursor, or agy
    Setup,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = Client::new();

    match &cli.command {
        Commands::Up { file, fuel } => match file {
            Some(path) => tet_cli::run::up_artifact(path, *fuel).await,
            None => tet_core::server::start::start(tet_core::config::Config::from_env()).await,
        },
        Commands::Run {
            payload_path,
            alias,
            fuel,
            memory,
        } => tet_cli::run::run_payload(&client, payload_path, alias, *fuel, *memory).await,
        Commands::Snapshot { alias, tag } => tet_cli::snapshot::snapshot(&client, alias, tag).await,
        Commands::Push { path, alias } => tet_cli::registry::push(path, alias).await,
        Commands::Ps => tet_cli::status::ps_cmd(&client, cli.json).await,
        Commands::Logs { alias } => {
            tet_cli::status::logs_cmd(&client, alias.as_deref(), cli.json).await
        }
        Commands::Metrics => tet_cli::status::metrics_cmd(&client, cli.json).await,
        Commands::Exec { code, lang, fuel } => {
            let (cid, fname) = match lang.as_str() {
                "py" | "python" => ("python-evaluator", "python_evaluator.wasm"),
                _ => ("js-evaluator", "js_evaluator.wasm"),
            };

            // Initialize a minimal sandbox — no mesh, no payment, local-only
            let sandbox = std::sync::Arc::new(tet_core::sandbox::WasmtimeSandbox::new(
                tet_core::mesh::TetMesh::new(1, tet_core::hive::HivePeers::new()).0,
                std::sync::Arc::new(tet_core::economy::VoucherManager::new("local".into())),
                false,
                "local".into(),
            )?);

            // Load the cartridge from dist/cartridges/
            let search_paths = &[
                std::env::current_dir()?.join("dist/cartridges").join(fname),
                std::env::current_dir()?
                    .join("crates")
                    .join(cid)
                    .join("target/wasm32-wasip1/release")
                    .join(fname),
            ];
            let wasm = search_paths
                .iter()
                .find_map(|p| std::fs::read(p).ok())
                .unwrap_or_else(|| {
                    panic!(
                        "{} not found. Build with: cargo component build --release -p {}",
                        fname, cid
                    )
                });

            sandbox
                .cartridge_manager
                .precompile(cid, &wasm)
                .map_err(|e| anyhow::anyhow!("{}", e))?;

            match sandbox.cartridge_manager.invoke(cid, code, *fuel, 64) {
                Ok((output, metrics)) => {
                    println!("{}", output);
                    eprintln!(
                        "fuel: {} instr  duration: {}µs",
                        metrics.fuel_consumed, metrics.duration_us
                    );
                }
                Err(tet_core::cartridge::CartridgeError::FuelExhausted) => {
                    eprintln!("trapped: fuel exhausted ({} instructions)", fuel);
                    std::process::exit(1);
                }
                Err(tet_core::cartridge::CartridgeError::MemoryExceeded) => {
                    eprintln!("trapped: memory limit exceeded");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("error: {}", e);
                    std::process::exit(1);
                }
            }
            Ok(())
        }
        Commands::Mcp { list_tools } => tet_cli::mcp::mcp_cmd(*list_tools).await,
        Commands::Serve => {
            let config = tet_core::config::Config::from_env();
            tet_core::server::start::start(config).await
        }
        Commands::Replay {
            snapshot_id,
            payload,
        } => tet_cli::run::replay_cmd(&client, snapshot_id, payload.as_deref()).await,
        Commands::Build { entry, out } => tet_cli::run::build_cmd(entry, out).await,
        Commands::Publish { path, tag } => {
            tet_cli::cartridge::publish_cmd(&path.to_string_lossy(), tag).await
        }
        Commands::Search { query } => tet_cli::cartridge::search_cmd(query).await,
        Commands::Validate { path } => {
            tet_cli::cartridge::validate_cmd(&path.to_string_lossy()).await
        }
        Commands::KeyCreate { label } => tet_cli::keys::create_key(label),
        Commands::KeyList => tet_cli::keys::list_keys(),
        Commands::KeyRevoke { prefix } => tet_cli::keys::revoke_key(prefix),
        Commands::Init { name } => {
            let project = name.as_deref().unwrap_or("trytet-agent");
            std::fs::create_dir_all(project)?;
            std::fs::write(
                format!("{}/agent.ts", project),
                "// Trytet agent entry point\nconsole.log(\"Hello from Trytet!\");\n",
            )?;
            std::fs::write(
                format!("{}/tet.toml", project),
                "[agent]\nname = \"my-agent\"\nversion = \"0.1.0\"\n",
            )?;
            println!("{} Initialized project '{}'", "✅".green(), project.cyan());
            println!("  cd {}", project);
            println!("  tet build agent.ts -o agent.tet");
            println!("  tet up agent.tet --fuel 1000000");
            Ok(())
        }
        Commands::Setup => {
            let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("tet"));
            let exe_str = exe.to_string_lossy();

            let home = home::home_dir();
            let mut configs: Vec<(&str, std::path::PathBuf)> = Vec::new();

            if let Some(ref h) = home {
                configs.push((
                    "Claude Desktop",
                    h.join("Library")
                        .join("Application Support")
                        .join("Claude")
                        .join("claude_desktop_config.json"),
                ));
                configs.push(("Cursor", h.join(".cursor").join("mcp.json")));
                configs.push(("agy", h.join(".agy").join("mcp.json")));
            }

            for (name, status) in tet_cli::setup::register_mcp(&exe_str, &configs) {
                match status.as_str() {
                    "already_configured" => {
                        println!(
                            "{} Trytet already configured in {}",
                            "✓".green(),
                            name.cyan()
                        )
                    }
                    "added" => println!("{} Added Trytet to {}", "✓".green(), name.cyan()),
                    _ => println!("{} Could not configure {}", "⚠".yellow(), name.cyan()),
                }
            }

            println!();
            println!("Restart your editor for the changes to take effect.");
            Ok(())
        }
        Commands::Doctor => {
            println!("🩺 Trytet Doctor v0.2.1\n");

            // Binary location
            match std::env::current_exe() {
                Ok(exe) => println!("  ✅ Binary: {}", exe.display()),
                Err(_) => println!("  ⚠️  Binary: unknown location"),
            }

            // Cartridge directory
            let home = home::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
            let cartridge_dir = std::env::var("TRYTET_CARTRIDGE_DIR")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| home.join(".trytet").join("cartridges"));

            print!(
                "  {} Cartridge dir: {} ",
                if cartridge_dir.exists() {
                    "✅"
                } else {
                    "⚠️ "
                },
                cartridge_dir.display()
            );
            if cartridge_dir.exists() {
                let count = std::fs::read_dir(&cartridge_dir)
                    .map(|d| {
                        d.filter(|e| {
                            e.as_ref()
                                .map(|f| f.path().extension().is_some_and(|ext| ext == "wasm"))
                                .unwrap_or(false)
                        })
                        .count()
                    })
                    .unwrap_or(0);
                println!("({} .wasm files)", count);
            } else {
                println!("(missing — run: mkdir -p ~/.trytet/cartridges)");
            }

            // Check for cartridges next to the binary (tarball install)
            if let Ok(exe) = std::env::current_exe() {
                if let Some(parent) = exe.parent() {
                    let sibling = parent.join("cartridges");
                    if sibling.exists() {
                        let count = std::fs::read_dir(&sibling).map(|d| d.count()).unwrap_or(0);
                        println!(
                            "  ✅ Binary-relative cartridges: {} ({} .wasm files)",
                            sibling.display(),
                            count
                        );
                    }
                }
            }

            // Check for cartridges in dist/ (dev build)
            if let Ok(cwd) = std::env::current_dir() {
                let dev_dir = cwd.join("dist").join("cartridges");
                if dev_dir.exists() {
                    let count = std::fs::read_dir(&dev_dir).map(|d| d.count()).unwrap_or(0);
                    println!("  ℹ️  Dev cartridges: {} ({})", dev_dir.display(), count);
                }
            }

            // Rust toolchain
            print!(
                "  {} Rust: ",
                if std::process::Command::new("rustc")
                    .arg("--version")
                    .output()
                    .is_ok()
                {
                    "✅"
                } else {
                    "❌"
                }
            );
            if let Ok(out) = std::process::Command::new("rustc")
                .arg("--version")
                .output()
            {
                print!("{}", String::from_utf8_lossy(&out.stdout).trim());
            }
            println!();

            // Cargo-component
            if std::process::Command::new("cargo")
                .arg("component")
                .arg("--version")
                .output()
                .is_ok()
            {
                print!("  ✅ cargo-component: ");
                if let Ok(out) = std::process::Command::new("cargo")
                    .arg("component")
                    .arg("--version")
                    .output()
                {
                    print!("{}", String::from_utf8_lossy(&out.stdout).trim());
                }
                println!();
            } else {
                println!(
                    "  ⚠️  cargo-component not found (needed to build cartridges from source)"
                );
            }

            // Configuration summary (redacted secrets)
            println!();
            let config = tet_core::config::Config::from_env();
            config.print_doctor();

            println!("\n  MCP config for Claude Desktop (~/Library/Application Support/Claude/claude_desktop_config.json):");
            println!("  {{");
            println!("    \"mcpServers\": {{");
            println!("      \"trytet\": {{");
            println!(
                "        \"command\": \"{}\",",
                std::env::current_exe()
                    .map(|e| e.display().to_string())
                    .unwrap_or_else(|_| "tet".into())
            );
            println!("        \"args\": [\"mcp\"]");
            println!("      }}");
            println!("    }}");
            println!("  }}");

            Ok(())
        }
    }
}

//! Tet Core Engine — Main Entrypoint
//!
//! Starts the API server on 0.0.0.0:3000.
//! Equivalent to `tet serve`.

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = tet_core::config::Config::from_env();
    tet_core::server::start::start(config).await
}

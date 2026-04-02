use std::path::PathBuf;
use std::time::{SystemTime, Duration};
use tokio::time;

pub async fn spawn_purge_thread() {
    tokio::spawn(async move {
        let registry_path = std::env::var("REGISTRY_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home_dir = home::home_dir().unwrap_or_else(|| PathBuf::from("."));
                home_dir.join(".trytet").join("registry")
            });

        let mut interval = time::interval(Duration::from_secs(3600)); // Run hourly

        loop {
            interval.tick().await;
            tracing::info!("Running zero-residue purge on {}", registry_path.display());

            if let Ok(entries) = std::fs::read_dir(&registry_path) {
                let now = SystemTime::now();
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(age) = now.duration_since(modified) {
                                if age.as_secs() > 24 * 3600 {
                                    if let Err(e) = std::fs::remove_file(entry.path()) {
                                        tracing::warn!("Failed to purge file {:?}: {}", entry.path(), e);
                                    } else {
                                        tracing::info!("Purged stale snapshot: {:?}", entry.path());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}

pub mod cache;
pub mod identity;
pub mod oci;
pub mod quorum;

pub use cache::*;
pub use identity::*;
pub use oci::*;
pub use quorum::*;

use std::fs;
use std::path::PathBuf;

pub trait Registry: Send + Sync {
    fn push(&self, tag: &str, payload: &[u8]) -> anyhow::Result<()>;
    fn pull(&self, tag: &str) -> anyhow::Result<Option<Vec<u8>>>;
}

pub struct LocalRegistry {
    storage_dir: PathBuf,
}

impl LocalRegistry {
    /// Create a new local registry rooted at `storage_dir`.
    ///
    /// Generally you want the path from [`Config::registry_path`]; the
    /// parameterless [`LocalRegistry::new_default`] variant is kept for
    /// legacy convenience callers.
    pub fn new(storage_dir: PathBuf) -> anyhow::Result<Self> {
        if !storage_dir.exists() {
            fs::create_dir_all(&storage_dir)?;
        }
        Ok(Self { storage_dir })
    }

    /// Create a local registry using the legacy home-dir default path.
    #[deprecated(
        since = "0.3.0",
        note = "use LocalRegistry::new(config.registry_path) instead"
    )]
    pub fn new_default() -> anyhow::Result<Self> {
        let home_dir = home::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self::new(home_dir.join(".trytet").join("registry"))
    }

    fn tag_to_path(&self, tag: &str) -> PathBuf {
        let safe_tag = tag.replace("/", "_").replace(":", "_");
        self.storage_dir.join(format!("{}.tet", safe_tag))
    }
}

impl Registry for LocalRegistry {
    fn push(&self, tag: &str, payload: &[u8]) -> anyhow::Result<()> {
        let path = self.tag_to_path(tag);
        fs::write(path, payload)?;
        Ok(())
    }

    fn pull(&self, tag: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let path = self.tag_to_path(tag);
        if path.exists() {
            Ok(Some(fs::read(path)?))
        } else {
            Ok(None)
        }
    }
}

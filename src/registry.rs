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
    pub fn new() -> anyhow::Result<Self> {
        let home_dir = home::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let storage_dir = home_dir.join(".trytet").join("registry");
        
        if !storage_dir.exists() {
            fs::create_dir_all(&storage_dir)?;
        }
        
        Ok(Self { storage_dir })
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

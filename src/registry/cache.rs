use anyhow::Result;
use std::fs;
use std::path::PathBuf;

pub struct ArtifactCache {
    pub root_dir: PathBuf,
}

impl ArtifactCache {
    pub fn new() -> Result<Self> {
        let home_dir = home::home_dir().expect("Could not find home directory");
        let root_dir = home_dir.join(".tet").join("cache");
        fs::create_dir_all(root_dir.join("blobs"))?;
        Ok(Self { root_dir })
    }

    pub fn get_blob_path(&self, digest: &str) -> PathBuf {
        // Ensure digest is just the hash part if it contains sha256: prefix
        let hash = digest.strip_prefix("sha256:").unwrap_or(digest);
        self.root_dir.join("blobs").join(hash)
    }

    pub fn blob_exists(&self, digest: &str) -> bool {
        self.get_blob_path(digest).exists()
    }

    pub fn store_blob(&self, digest: &str, data: &[u8]) -> Result<()> {
        let path = self.get_blob_path(digest);
        fs::write(path, data)?;
        Ok(())
    }

    pub fn read_blob(&self, digest: &str) -> Result<Vec<u8>> {
        let path = self.get_blob_path(digest);
        Ok(fs::read(path)?)
    }

    pub fn link_tag(&self, reference: &str, manifest_digest: &str) -> Result<()> {
        let tags_dir = self.root_dir.join("tags");
        fs::create_dir_all(&tags_dir)?;
        let safe_ref = reference.replace("/", "_").replace(":", "_");
        fs::write(tags_dir.join(safe_ref), manifest_digest)?;
        Ok(())
    }

    pub fn resolve_tag(&self, reference: &str) -> Result<Option<String>> {
        let safe_ref = reference.replace("/", "_").replace(":", "_");
        let tag_path = self.root_dir.join("tags").join(safe_ref);
        if tag_path.exists() {
            let digest = fs::read_to_string(tag_path)?;
            Ok(Some(digest))
        } else {
            Ok(None)
        }
    }
}

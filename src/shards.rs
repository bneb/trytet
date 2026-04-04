use dashmap::{DashMap, DashSet};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use crate::memory::{VectorShard, NUM_SHARDS};

use std::sync::OnceLock;

/// Global RefStore tracking active layers across all agents on this Node.
fn global_ref_store() -> &'static DashMap<Uuid, Arc<AtomicU64>> {
    static STORE: OnceLock<DashMap<Uuid, Arc<AtomicU64>>> = OnceLock::new();
    STORE.get_or_init(DashMap::new)
}

/// A Serializable reference count. Ensures multiple handles to the same VFS layer
/// correctly share the same atomic counter within the host process.
#[derive(Clone, Debug)]
pub struct LayerRef(pub Arc<AtomicU64>);

impl Serialize for LayerRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.0.load(Ordering::SeqCst))
    }
}

impl<'de> Deserialize<'de> for LayerRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let _val = u64::deserialize(deserializer)?;
        Ok(LayerRef(Arc::new(AtomicU64::new(1))))
    }
}

#[derive(Serialize, Deserialize)]
pub struct VfsLayer {
    pub id: Uuid,
    pub path: PathBuf,
    pub is_readonly: bool,
    #[serde(skip)] // Hydrate manually via ID
    pub ref_count: Option<Arc<AtomicU64>>,

    // In-Memory state. If set to None, we must load from Zstd 'path'.
    // If it's active, it's always Some().
    #[serde(skip)]
    pub memory_shards: Option<Vec<Arc<VectorShard>>>,

    // Bloom Filter / Tombstone hashset for deletes
    pub tombstones: DashSet<String>,
}

impl Clone for VfsLayer {
    fn clone(&self) -> Self {
        self.increment_ref();
        Self {
            id: self.id,
            path: self.path.clone(),
            is_readonly: self.is_readonly,
            ref_count: self.ref_count.clone(),
            memory_shards: self.memory_shards.clone(),
            tombstones: self.tombstones.clone(),
        }
    }
}

impl VfsLayer {
    pub fn new(path: PathBuf, is_readonly: bool, shards: Vec<Arc<VectorShard>>) -> Self {
        let id = Uuid::new_v4();
        let ref_count = Arc::new(AtomicU64::new(1));
        global_ref_store().insert(id, ref_count.clone());

        Self {
            id,
            path,
            is_readonly,
            ref_count: Some(ref_count),
            memory_shards: Some(shards),
            tombstones: DashSet::new(),
        }
    }

    pub fn load_from_disk(&mut self) -> Result<(), anyhow::Error> {
        if self.memory_shards.is_none() {
            let compressed = std::fs::read(&self.path)?;
            let bytes = zstd::decode_all(compressed.as_slice())?;
            let (shards, tb): (Vec<Arc<VectorShard>>, DashSet<String>) =
                bincode::deserialize(&bytes)?;
            self.memory_shards = Some(shards);
            self.tombstones = tb;
        }
        Ok(())
    }

    pub fn hydrate_ref(&mut self) {
        let global_ref = global_ref_store()
            .entry(self.id)
            .or_insert_with(|| Arc::new(AtomicU64::new(1)));
        self.ref_count = Some(global_ref.clone());
    }

    pub fn increment_ref(&self) {
        if let Some(rc) = &self.ref_count {
            rc.fetch_add(1, Ordering::SeqCst);
        }
    }

    pub fn decrement_ref(&self) -> u64 {
        if let Some(rc) = &self.ref_count {
            let previous = rc.fetch_sub(1, Ordering::SeqCst);
            if previous > 0 {
                return previous - 1;
            } else {
                return 0; // prevent underflow
            }
        }
        0
    }

    pub fn serialize_to_disk(&self) {
        if !self.is_readonly || self.memory_shards.is_none() {
            return;
        }
        // Serialize zstd compression off main thread
        let shards = self.memory_shards.as_ref().unwrap().clone();
        let p = self.path.clone();
        let tb = self.tombstones.clone();

        std::thread::spawn(move || {
            if let Ok(bytes) = bincode::serialize(&(shards, tb)) {
                if let Ok(compressed) = zstd::encode_all(bytes.as_slice(), 3) {
                    let _ = std::fs::write(p, compressed);
                }
            }
        });
    }
}

impl Drop for VfsLayer {
    fn drop(&mut self) {
        if self.is_readonly {
            let remain = self.decrement_ref();
            if remain == 0 {
                // Garbage Collection: Delete physical shard off disk!
                let _ = std::fs::remove_file(&self.path);
                global_ref_store().remove(&self.id);
            }
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct LayeredVectorStore {
    pub base_layers: Vec<VfsLayer>, // Immutable ancestors
    pub active_layer: VfsLayer,     // Current mutable delta
}

impl Default for LayeredVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

impl LayeredVectorStore {
    pub fn new() -> Self {
        let host_dir = home::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".trytet")
            .join("vfs_layers");
        std::fs::create_dir_all(&host_dir).unwrap_or_default();

        let active_path = host_dir.join(format!("{}.zst", Uuid::new_v4()));

        let mut shards = Vec::with_capacity(NUM_SHARDS);
        for _ in 0..NUM_SHARDS {
            shards.push(Arc::new(VectorShard::default()));
        }

        Self {
            base_layers: Vec::new(),
            active_layer: VfsLayer::new(active_path, false, shards),
        }
    }
}

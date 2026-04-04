use ahash::AHasher;
use dashmap::DashMap;
use instant_distance::{Builder, HnswMap, Point, Search};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VectorRecord {
    pub id: String,
    pub vector: Vec<f32>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchQuery {
    pub collection: String,
    pub query_vector: Vec<f32>,
    pub limit: u32,
    pub min_score: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub metadata: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CosinePoint(pub Vec<f32>);

impl Eq for CosinePoint {}

impl Point for CosinePoint {
    fn distance(&self, other: &Self) -> f32 {
        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;

        for (a, b) in self.0.iter().zip(other.0.iter()) {
            dot += a * b;
            norm_a += a * a;
            norm_b += b * b;
        }

        let sim = if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot / (norm_a.sqrt() * norm_b.sqrt())
        };

        // Return cosine distance bounds [0, 2]
        1.0 - sim
    }
}

// ---------------------------------------------------------------------------
// Native Vector Storage Layer (Tiered LSM-Vector Hybrid)
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
pub struct Tier1Data {
    pub records: DashMap<String, VectorRecord>,
}

impl Default for Tier1Data {
    fn default() -> Self {
        Self {
            records: DashMap::new(),
        }
    }
}

impl Clone for Tier1Data {
    fn clone(&self) -> Self {
        let new_map = DashMap::new();
        for kv in self.records.iter() {
            new_map.insert(kv.key().clone(), kv.value().clone());
        }
        Self { records: new_map }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct Tier2Data {
    pub records: Vec<VectorRecord>,
    pub points: Vec<CosinePoint>,
    #[serde(skip)]
    pub index: Option<HnswMap<CosinePoint, String>>,
}

use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct VectorCollection {
    pub tier1: Tier1Data,
    pub tier2: Arc<std::sync::RwLock<Arc<Tier2Data>>>,
    #[serde(skip)]
    pub is_compacting: Arc<AtomicBool>,
}

pub const NUM_SHARDS: usize = 32;

#[derive(Serialize, Deserialize, Clone)]
pub struct VectorShard {
    pub collections: DashMap<String, Arc<VectorCollection>>,
}

impl Default for VectorShard {
    fn default() -> Self {
        Self {
            collections: DashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct VectorVfs {
    pub store: crate::shards::LayeredVectorStore,
    #[serde(skip)]
    pub compaction_tx: Option<tokio::sync::mpsc::UnboundedSender<String>>,
}

impl Default for VectorVfs {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorVfs {
    pub fn new() -> Self {
        let mut vfs = Self {
            store: crate::shards::LayeredVectorStore::new(),
            compaction_tx: None,
        };
        vfs.start_background_worker();
        vfs
    }

    /// Create a CoW clone of this VFS for a child agent
    pub fn spawn_cow_child(&mut self, child_id: &str) -> Result<Self, crate::engine::TetError> {
        // 1. Mark current 'active_layer' as READ-ONLY
        self.store.active_layer.is_readonly = true;
        self.store.active_layer.serialize_to_disk();

        // 2. Both parent and child push this finalized layer to their base_layers
        let frozen_layer = self.store.active_layer.clone(); // This uses our custom Clone which increments RefCount
        self.store.base_layers.push(frozen_layer.clone());

        let mut child_store = crate::shards::LayeredVectorStore {
            base_layers: self.store.base_layers.clone(),
            active_layer: self.store.active_layer.clone(), // placeholder
        };

        let host_dir = home::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join(".trytet")
            .join("vfs_layers");

        // 3. Create fresh 'active_layer' (Delta) for the Parent
        let parent_active_path = host_dir.join(format!("{}_parent.zst", uuid::Uuid::new_v4()));
        let mut p_shards = Vec::with_capacity(NUM_SHARDS);
        for _ in 0..NUM_SHARDS {
            p_shards.push(Arc::new(VectorShard::default()));
        }
        self.store.active_layer = crate::shards::VfsLayer::new(parent_active_path, false, p_shards);

        // 4. Create fresh 'active_layer' (Delta) for the Child
        let child_active_path = host_dir.join(format!("{}_{}.zst", uuid::Uuid::new_v4(), child_id));
        let mut c_shards = Vec::with_capacity(NUM_SHARDS);
        for _ in 0..NUM_SHARDS {
            c_shards.push(Arc::new(VectorShard::default()));
        }
        child_store.active_layer = crate::shards::VfsLayer::new(child_active_path, false, c_shards);

        let mut vfs = Self {
            store: child_store,
            compaction_tx: None,
        };
        vfs.start_background_worker();

        Ok(vfs)
    }

    pub fn forget(&self, record_id: &str) {
        self.store
            .active_layer
            .tombstones
            .insert(record_id.to_string());
    }

    pub fn start_background_worker(&mut self) {
        if self.compaction_tx.is_some() {
            return;
        }
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        self.compaction_tx = Some(tx);
        let shards_clone = match self.store.active_layer.memory_shards.as_ref() {
            Some(s) => s.clone(),
            None => return,
        };

        tokio::spawn(async move {
            while let Some(collection) = rx.recv().await {
                let mut hasher = AHasher::default();
                std::hash::Hash::hash(&collection, &mut hasher);
                let idx = (std::hash::Hasher::finish(&hasher) as usize) % NUM_SHARDS;
                let shard = shards_clone[idx].clone();
                let col = match shard.collections.get(&collection) {
                    Some(c) => c.clone(),
                    None => continue,
                };
                tokio::task::spawn_blocking(move || {
                    Self::do_compaction(&col);
                })
                .await
                .unwrap();
            }
        });
    }

    fn get_shard(&self, collection: &str) -> Arc<VectorShard> {
        let mut hasher = AHasher::default();
        collection.hash(&mut hasher);
        let idx = (hasher.finish() as usize) % NUM_SHARDS;
        self.store.active_layer.memory_shards.as_ref().unwrap()[idx].clone()
    }

    fn get_shard_from_layer(
        &self,
        layer: &crate::shards::VfsLayer,
        collection: &str,
    ) -> Option<Arc<VectorShard>> {
        if let Some(shards) = &layer.memory_shards {
            let mut hasher = AHasher::default();
            collection.hash(&mut hasher);
            let idx = (hasher.finish() as usize) % NUM_SHARDS;
            return Some(shards[idx].clone());
        }
        None
    }

    fn get_collection(&self, collection: &str) -> Arc<VectorCollection> {
        let shard = self.get_shard(collection);
        let res = shard
            .collections
            .entry(collection.to_string())
            .or_insert_with(|| {
                Arc::new(VectorCollection {
                    tier1: Tier1Data::default(),
                    tier2: Arc::new(std::sync::RwLock::new(Arc::new(Tier2Data::default()))),
                    is_compacting: Arc::new(AtomicBool::new(false)),
                })
            })
            .value()
            .clone();
        res
    }

    /// Appends the vector into memory via a Tier 1 O(1) buffer.
    pub fn remember(&self, collection: &str, record: VectorRecord) {
        let col = self.get_collection(collection);

        // Enforce 2048 hard cap to prevent unbounded Memory Bombs
        if col.tier1.records.len() >= 2048 {
            self.compact_collection(collection);
        }

        self.store.active_layer.tombstones.remove(&record.id);
        col.tier1.records.insert(record.id.clone(), record);
    }

    /// Performs sub-millisecond similarity graph traversal bridging Tiers 1 and 2, and layers.
    pub fn recall(&self, query: &SearchQuery) -> Vec<SearchResult> {
        let qp = CosinePoint(query.query_vector.clone());
        let mut out = Vec::new();

        let mut layers = Vec::new();
        layers.push(&self.store.active_layer);
        for base in self.store.base_layers.iter().rev() {
            layers.push(base);
        }

        let mut seen = std::collections::HashSet::new();

        for layer in layers {
            for tb in layer.tombstones.iter() {
                seen.insert(tb.key().clone());
            }

            if let Some(shard) = self.get_shard_from_layer(layer, &query.collection) {
                if let Some(col) = shard.collections.get(&query.collection) {
                    // Scan Tier 1
                    for kv in col.tier1.records.iter() {
                        let record = kv.value();
                        if seen.contains(&record.id) {
                            continue;
                        }

                        let rp = CosinePoint(record.vector.clone());
                        let score = 1.0 - qp.distance(&rp);
                        if score >= query.min_score {
                            out.push((score, record.clone()));
                            seen.insert(record.id.clone());
                        }
                    }

                    // Scan Tier 2
                    let t2 = col.tier2.read().unwrap().clone();
                    if let Some(idx) = &t2.index {
                        let mut search = Search::default();
                        let results = idx.search(&qp, &mut search);

                        for r in results.take((query.limit * 5) as usize) {
                            let score = 1.0 - r.distance;
                            if score >= query.min_score
                                && !seen.contains(r.value) {
                                    if let Some(record) =
                                        t2.records.iter().find(|rec| rec.id == *r.value)
                                    {
                                        out.push((score, record.clone()));
                                        seen.insert(record.id.clone());
                                    }
                                }
                        }
                    } else if !t2.records.is_empty() {
                        for r in &t2.records {
                            if seen.contains(&r.id) {
                                continue;
                            }
                            let rp = CosinePoint(r.vector.clone());
                            let score = 1.0 - qp.distance(&rp);
                            if score >= query.min_score {
                                out.push((score, r.clone()));
                                seen.insert(r.id.clone());
                            }
                        }
                    }
                }
            }
        }

        out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        out.into_iter()
            .take(query.limit as usize)
            .map(|(score, r)| SearchResult {
                id: r.id,
                score,
                metadata: r.metadata,
            })
            .collect()
    }

    pub fn compact_collection(&self, collection: &str) {
        let col = self.get_collection(collection);
        if let Some(tx) = &self.compaction_tx {
            if !col.is_compacting.swap(true, Ordering::SeqCst) {
                let _ = tx.send(collection.to_string());
            }
        } else {
            Self::do_compaction(&col);
        }
    }

    fn do_compaction(col: &Arc<VectorCollection>) {
        let mut new_records = Vec::new();
        // Remove processed entries from Tier 1 thread-safely
        col.tier1.records.retain(|_, v| {
            new_records.push(v.clone());
            false // Drop from Dashmap
        });

        if new_records.is_empty() {
            col.is_compacting.store(false, Ordering::SeqCst);
            return;
        }

        let mut points = Vec::new();
        let mut records = Vec::new();
        {
            let t2 = col.tier2.read().unwrap().clone();
            points.extend(t2.points.clone());
            records.extend(t2.records.clone());
        }

        for r in new_records {
            points.push(CosinePoint(r.vector.clone()));
            records.push(r.clone());
        }

        let v_clone: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
        let new_idx = Builder::default().build(points.clone(), v_clone);

        // Atomic immutable swap into Tier 2 via Arc replacement
        let new_tier2 = Arc::new(Tier2Data {
            points,
            records,
            index: Some(new_idx),
        });
        let mut t2_write = col.tier2.write().unwrap();
        *t2_write = new_tier2;
        col.is_compacting.store(false, Ordering::SeqCst);
    }

    pub fn rebuild_all_indexes(&self) {
        if let Some(shards) = &self.store.active_layer.memory_shards {
            for shard in shards {
                let cols: Vec<String> = shard
                    .collections
                    .iter()
                    .map(|col| col.key().clone())
                    .collect();
                for col in cols {
                    self.compact_collection(&col);
                }
            }
        }
    }
}

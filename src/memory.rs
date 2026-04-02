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

#[derive(Serialize, Deserialize, Default, Clone)]
pub struct VectorCollection {
    pub tier1: Tier1Data,
    pub tier2: Arc<std::sync::RwLock<Tier2Data>>,
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
    pub shards: Vec<Arc<VectorShard>>,
}

impl Default for VectorVfs {
    fn default() -> Self {
        Self::new()
    }
}

impl VectorVfs {
    pub fn new() -> Self {
        let mut shards = Vec::with_capacity(NUM_SHARDS);
        for _ in 0..NUM_SHARDS {
            shards.push(Arc::new(VectorShard::default()));
        }
        Self { shards }
    }

    fn get_shard(&self, collection: &str) -> Arc<VectorShard> {
        let mut hasher = AHasher::default();
        collection.hash(&mut hasher);
        let idx = (hasher.finish() as usize) % NUM_SHARDS;
        self.shards[idx].clone()
    }

    fn get_collection(&self, collection: &str) -> Arc<VectorCollection> {
        let shard = self.get_shard(collection);
        let res = shard
            .collections
            .entry(collection.to_string())
            .or_insert_with(|| Arc::new(VectorCollection::default()))
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
        
        col.tier1.records.insert(record.id.clone(), record);
    }

    /// Performs sub-millisecond similarity graph traversal bridging Tiers 1 and 2.
    pub fn recall(&self, query: &SearchQuery) -> Vec<SearchResult> {
        let col = self.get_collection(&query.collection);
        let qp = CosinePoint(query.query_vector.clone());

        let mut out = Vec::new();

        // Scan Tier 1 (brute-force on small hot append buffer)
        for kv in col.tier1.records.iter() {
            let record = kv.value();
            let rp = CosinePoint(record.vector.clone());
            let score = 1.0 - qp.distance(&rp);
            if score >= query.min_score {
                out.push((score, record.clone()));
            }
        }

        // Scan Tier 2 HNSW Immutable geometric index
        let t2 = col.tier2.read().unwrap();
        if let Some(idx) = &t2.index {
            let mut search = Search::default();
            let results = idx.search(&qp, &mut search);

            for r in results.take(query.limit as usize) {
                let score = 1.0 - r.distance;
                if score >= query.min_score {
                    if let Some(record) = t2.records.iter().find(|rec| rec.id == *r.value) {
                        // Prevent duplicate returns if compacting
                        if !out.iter().any(|(_, t1_rec)| t1_rec.id == record.id) {
                            out.push((score, record.clone()));
                        }
                    }
                }
            }
        } else if !t2.records.is_empty() {
            for r in &t2.records {
                let rp = CosinePoint(r.vector.clone());
                let score = 1.0 - qp.distance(&rp);
                if score >= query.min_score && !out.iter().any(|(_, t1_rec)| t1_rec.id == r.id) {
                    out.push((score, r.clone()));
                }
            }
        }

        out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        out.into_iter()
            .take(query.limit as usize)
            .map(|(s, r)| SearchResult {
                id: r.id.clone(),
                score: s,
                metadata: r.metadata.clone(),
            })
            .collect()
    }

    /// Merges Tier 1 into Tier 2 via a full rebuilt immutable graph, swapping pointers on finish.
    pub fn compact_collection(&self, collection: &str) {
        let col = self.get_collection(collection);

        let mut new_records = Vec::new();
        // Remove processed entries from Tier 1 thread-safely
        col.tier1.records.retain(|_, v| {
            new_records.push(v.clone());
            false // Drop from Dashmap
        });

        if new_records.is_empty() {
            return;
        }

        let mut points = Vec::new();
        let mut records = Vec::new();
        {
            let t2 = col.tier2.read().unwrap();
            points.extend(t2.points.clone());
            records.extend(t2.records.clone());
        }

        for r in new_records {
            points.push(CosinePoint(r.vector.clone()));
            records.push(r.clone());
        }

        let v_clone: Vec<String> = records.iter().map(|r| r.id.clone()).collect();
        let new_idx = Builder::default().build(points.clone(), v_clone);

        // Atomic immutable swap into Tier 2
        let mut t2_write = col.tier2.write().unwrap();
        t2_write.points = points;
        t2_write.records = records;
        t2_write.index = Some(new_idx);
    }

    pub fn rebuild_all_indexes(&self) {
        for shard in &self.shards {
            let cols: Vec<String> = shard.collections.iter().map(|col| col.key().clone()).collect();
            for col in cols {
                self.compact_collection(&col);
            }
        }
    }
}

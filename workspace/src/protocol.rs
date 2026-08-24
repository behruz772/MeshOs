use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncAction {
    Upsert,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRecord {
    pub action: SyncAction,
    pub path: String,
    pub size: u64,
    pub sha256: Option<String>,
    pub event_id: String,
}

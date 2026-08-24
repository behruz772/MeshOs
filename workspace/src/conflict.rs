use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVersion {
    pub path: String,
    pub version: u64,
    pub sha256: String,
    pub device_id: String,
    pub event_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Resolution {
    AcceptRemote,
    KeepLocal,
    Conflict,
}

pub fn resolve(local: &FileVersion, remote: &FileVersion) -> Resolution {
    if local.sha256 == remote.sha256 {
        return Resolution::AcceptRemote;
    }

    if remote.version > local.version {
        return Resolution::AcceptRemote;
    }

    if local.version > remote.version {
        return Resolution::KeepLocal;
    }

    Resolution::Conflict
}

use crate::conflict::{FileVersion, Resolution, resolve};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictRecord {
    pub path: String,
    pub local: FileVersion,
    pub remote: FileVersion,
    pub reason: String,
}

pub struct ConflictStore {
    path: PathBuf,
}

impl ConflictStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let dir = root.as_ref().join(".meshos");
        fs::create_dir_all(&dir)?;

        Ok(Self {
            path: dir.join("conflicts.jsonl"),
        })
    }

    pub fn record(&self, conflict: &ConflictRecord) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        writeln!(file, "{}", serde_json::to_string(conflict)?)?;

        file.sync_all()?;
        Ok(())
    }

    pub fn resolve(
        &self,
        local: FileVersion,
        remote: FileVersion,
    ) -> Result<Resolution, Box<dyn std::error::Error>> {
        let resolution = resolve(&local, &remote);

        if resolution == Resolution::Conflict {
            self.record(&ConflictRecord {
                path: local.path.clone(),
                local,
                remote,
                reason: "same version, different content".into(),
            })?;
        }

        Ok(resolution)
    }
}

pub fn conflict_copy_path(
    root: impl AsRef<std::path::Path>,
    path: &str,
    device_id: &str,
    version: u64,
) -> std::path::PathBuf {
    let original = root.as_ref().join(path);

    let parent = original.parent().unwrap_or(root.as_ref());

    let name = original
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");

    parent.join(format!("{}.conflict-{}-v{}", name, device_id, version))
}

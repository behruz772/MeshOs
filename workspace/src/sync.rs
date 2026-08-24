use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub size: u64,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceManifest {
    pub workspace_id: Uuid,
    pub version: u64,
    pub files: Vec<FileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncOperation {
    Create,
    Modify,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncItem {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub path: String,
    pub operation: SyncOperation,
    pub version: u64,
}

pub struct SyncEngine {
    root: PathBuf,
    workspace_id: Uuid,
}

impl SyncEngine {
    pub fn new(root: impl Into<PathBuf>, workspace_id: Uuid) -> Self {
        Self {
            root: root.into(),
            workspace_id,
        }
    }

    pub fn build_manifest(
        &self,
        version: u64,
    ) -> Result<WorkspaceManifest, Box<dyn std::error::Error>> {
        let mut files = Vec::new();

        self.scan_directory(&self.root, &mut files)?;

        Ok(WorkspaceManifest {
            workspace_id: self.workspace_id,
            version,
            files,
        })
    }

    fn scan_directory(
        &self,
        directory: &Path,
        files: &mut Vec<FileEntry>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();

            if path.file_name().and_then(|n| n.to_str()) == Some(".meshos") {
                continue;
            }

            if path.is_dir() {
                self.scan_directory(&path, files)?;
                continue;
            }

            if path.is_file() {
                let metadata = fs::metadata(&path)?;
                let hash = Self::file_hash(&path)?;

                let relative = path
                    .strip_prefix(&self.root)?
                    .to_string_lossy()
                    .replace('\\', "/");

                files.push(FileEntry {
                    path: relative,
                    size: metadata.len(),
                    hash,
                });
            }
        }

        Ok(())
    }

    fn file_hash(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
        let data = fs::read(path)?;
        let mut hasher = Sha256::new();
        hasher.update(&data);

        Ok(hex::encode(hasher.finalize()))
    }

    pub fn queue_path(&self) -> PathBuf {
        self.root.join(".meshos").join("sync-queue.jsonl")
    }

    pub fn queue_operation(&self, item: &SyncItem) -> Result<(), Box<dyn std::error::Error>> {
        let meshos_dir = self.root.join(".meshos");
        fs::create_dir_all(&meshos_dir)?;

        let data = serde_json::to_string(item)?;

        use std::io::Write;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.queue_path())?;

        writeln!(file, "{data}")?;
        file.sync_data()?;

        Ok(())
    }
}

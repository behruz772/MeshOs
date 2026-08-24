use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;

#[derive(Debug, Clone)]
pub struct TransferJob {
    pub relative_path: String,
    pub absolute_path: PathBuf,
    pub size: u64,
    pub sha256: String,
}

pub async fn build_job(
    root: impl AsRef<Path>,
    relative_path: &str,
) -> Result<TransferJob, Box<dyn std::error::Error>> {
    let root = root.as_ref();
    let path = root.join(relative_path);

    let data = fs::read(&path).await?;
    let metadata = fs::metadata(&path).await?;

    let hash = Sha256::digest(&data);

    let sha256 = hash.iter().map(|b| format!("{b:02x}")).collect::<String>();

    Ok(TransferJob {
        relative_path: relative_path.to_string(),
        absolute_path: path,
        size: metadata.len(),
        sha256,
    })
}

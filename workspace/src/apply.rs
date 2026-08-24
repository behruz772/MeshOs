use crate::conflict::{FileVersion, Resolution, resolve};
use crate::conflicts::conflict_copy_path;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyResult {
    AppliedRemote,
    KeptLocal,
    ConflictCreated(PathBuf),
    AlreadyIdentical,
}

fn durable_write(path: &Path, data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;

    let mut file = std::fs::File::create(path)?;
    file.write_all(data)?;
    file.sync_all()?;

    Ok(())
}

pub fn apply_remote(
    root: impl AsRef<Path>,
    local: Option<&FileVersion>,
    remote: &FileVersion,
    remote_bytes: &[u8],
) -> Result<ApplyResult, Box<dyn std::error::Error>> {
    let root = root.as_ref();
    let destination = root.join(&remote.path);

    if let Some(local) = local {
        match resolve(local, remote) {
            Resolution::AcceptRemote => {
                if local.sha256 == remote.sha256 {
                    return Ok(ApplyResult::AlreadyIdentical);
                }

                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }

                durable_write(&destination, remote_bytes)?;
                return Ok(ApplyResult::AppliedRemote);
            }

            Resolution::KeepLocal => {
                return Ok(ApplyResult::KeptLocal);
            }

            Resolution::Conflict => {
                let conflict =
                    conflict_copy_path(root, &remote.path, &remote.device_id, remote.version);

                if let Some(parent) = conflict.parent() {
                    fs::create_dir_all(parent)?;
                }

                durable_write(&conflict, remote_bytes)?;

                return Ok(ApplyResult::ConflictCreated(conflict));
            }
        }
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }

    durable_write(&destination, remote_bytes)?;

    Ok(ApplyResult::AppliedRemote)
}

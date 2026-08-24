use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct WorkspaceChange {
    pub event_id: Uuid,
    pub workspace_id: Uuid,
    pub kind: String,
    pub relative_path: String,
    pub size: u64,
    pub sha256: Option<String>,
}

pub fn start(
    root: &Path,
) -> Result<(RecommendedWatcher, Receiver<Result<Event, notify::Error>>), Box<dyn std::error::Error>>
{
    fs::create_dir_all(root)?;

    let (tx, rx) = mpsc::channel();

    let mut watcher = RecommendedWatcher::new(
        move |result| {
            let _ = tx.send(result);
        },
        Config::default(),
    )?;

    watcher.watch(root, RecursiveMode::Recursive)?;

    Ok((watcher, rx))
}

pub fn to_change(root: &Path, workspace_id: Uuid, event: Event) -> Vec<WorkspaceChange> {
    let kind = match event.kind {
        EventKind::Create(_) => "created",
        EventKind::Modify(_) => "modified",
        EventKind::Remove(_) => "removed",
        _ => return Vec::new(),
    }
    .to_string();

    let mut changes = Vec::new();

    for path in event.paths {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };

        let relative_path = relative.to_string_lossy().replace('\\', "/");

        if relative_path == ".meshos"
            || relative_path.starts_with(".meshos/")
            || relative_path.is_empty()
        {
            continue;
        }

        let (size, sha256) = match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => {
                let size = metadata.len();

                let hash = fs::read(&path).ok().map(|data| {
                    let digest = Sha256::digest(&data);

                    digest
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                });

                (size, hash)
            }
            _ => (0, None),
        };

        changes.push(WorkspaceChange {
            event_id: Uuid::new_v4(),
            workspace_id,
            kind: kind.clone(),
            relative_path,
            size,
            sha256,
        });
    }

    changes
}

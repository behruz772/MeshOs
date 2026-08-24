pub mod sync;

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub root_path: String,
    pub owner_device: Uuid,
    pub version: u64,
}

impl Workspace {
    pub fn new(name: impl Into<String>, root_path: impl Into<String>, owner_device: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            root_path: root_path.into(),
            owner_device,
            version: 1,
        }
    }

    pub fn next_version(&mut self) {
        self.version += 1;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeKind {
    Created,
    Modified,
    Removed,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub path: String,
    pub kind: ChangeKind,
    pub version: u64,
}

pub struct SyncJournal {
    path: PathBuf,
}

impl SyncJournal {
    pub fn new(workspace: &Workspace) -> Result<Self, Box<dyn std::error::Error>> {
        let root = PathBuf::from(&workspace.root_path);
        fs::create_dir_all(&root)?;

        let journal_dir = root.join(".meshos");
        fs::create_dir_all(&journal_dir)?;

        Ok(Self {
            path: journal_dir.join("sync-journal.jsonl"),
        })
    }

    pub fn append(&self, record: &ChangeRecord) -> Result<(), Box<dyn std::error::Error>> {
        let serialized = serde_json::to_string(record)?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        writeln!(file, "{serialized}")?;
        file.sync_data()?;

        Ok(())
    }
}

pub struct WorkspaceWatcher {
    workspace: Workspace,
    _watcher: RecommendedWatcher,
}

impl WorkspaceWatcher {
    pub fn new(workspace: Workspace) -> Result<Self, Box<dyn std::error::Error>> {
        let journal = SyncJournal::new(&workspace)?;
        let workspace_id = workspace.id;
        let workspace_root = PathBuf::from(&workspace.root_path);
        let mut version = workspace.version;

        let mut watcher = RecommendedWatcher::new(
            move |result: Result<Event, notify::Error>| {
                let Ok(event) = result else {
                    return;
                };

                let kind = match event.kind {
                    EventKind::Create(_) => ChangeKind::Created,
                    EventKind::Modify(_) => ChangeKind::Modified,
                    EventKind::Remove(_) => ChangeKind::Removed,
                    _ => ChangeKind::Other,
                };

                for path in event.paths {
                    if path.starts_with(workspace_root.join(".meshos")) {
                        continue;
                    }

                    let relative_path = path
                        .strip_prefix(&workspace_root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");

                    version += 1;

                    let record = ChangeRecord {
                        id: Uuid::new_v4(),
                        workspace_id,
                        path: relative_path,
                        kind: kind.clone(),
                        version,
                    };

                    if let Err(error) = journal.append(&record) {
                        eprintln!("Failed to write sync journal: {error}");
                    }
                }
            },
            Config::default(),
        )?;

        watcher.watch(Path::new(&workspace.root_path), RecursiveMode::Recursive)?;

        Ok(Self {
            workspace,
            _watcher: watcher,
        })
    }

    pub fn workspace(&self) -> &Workspace {
        &self.workspace
    }
}

pub mod transfer;

pub mod queue;
pub mod watch;

pub mod protocol;

pub mod state;

#[cfg(test)]
mod state_tests {
    use super::conflict::*;
    use super::state::*;
    use tempfile::tempdir;

    #[test]
    fn state_persists_file_version() {
        let dir = tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();

        let version = FileVersion {
            path: "test.txt".into(),
            version: 1,
            sha256: "abc".into(),
            device_id: "A".into(),
            event_id: "event-1".into(),
        };

        let state = store.load().unwrap();
        store.upsert(state, version).unwrap();

        let loaded = store.load().unwrap();

        assert_eq!(loaded.files["test.txt"].sha256, "abc");
    }

    #[test]
    fn equal_content_is_not_conflict() {
        let a = FileVersion {
            path: "a".into(),
            version: 1,
            sha256: "same".into(),
            device_id: "A".into(),
            event_id: "1".into(),
        };

        let b = FileVersion {
            path: "a".into(),
            version: 7,
            sha256: "same".into(),
            device_id: "B".into(),
            event_id: "2".into(),
        };

        assert_eq!(resolve(&a, &b), Resolution::AcceptRemote);
    }

    #[test]
    fn newer_remote_wins() {
        let local = FileVersion {
            path: "a".into(),
            version: 2,
            sha256: "aaa".into(),
            device_id: "A".into(),
            event_id: "1".into(),
        };

        let remote = FileVersion {
            path: "a".into(),
            version: 3,
            sha256: "bbb".into(),
            device_id: "B".into(),
            event_id: "2".into(),
        };

        assert_eq!(resolve(&local, &remote), Resolution::AcceptRemote);
    }

    #[test]
    fn equal_version_different_content_conflicts() {
        let local = FileVersion {
            path: "a".into(),
            version: 5,
            sha256: "aaa".into(),
            device_id: "A".into(),
            event_id: "1".into(),
        };

        let remote = FileVersion {
            path: "a".into(),
            version: 5,
            sha256: "bbb".into(),
            device_id: "B".into(),
            event_id: "2".into(),
        };

        assert_eq!(resolve(&local, &remote), Resolution::Conflict);
    }
}

pub mod conflict;

#[cfg(test)]
mod conflict_store_tests {
    use super::conflict::FileVersion;
    use super::conflict::Resolution;
    use super::conflicts::ConflictStore;
    use tempfile::tempdir;

    #[test]
    fn conflict_is_persisted() {
        let dir = tempdir().unwrap();
        let store = ConflictStore::new(dir.path()).unwrap();

        let local = FileVersion {
            path: "test.txt".into(),
            version: 7,
            sha256: "LOCAL".into(),
            device_id: "A".into(),
            event_id: "1".into(),
        };

        let remote = FileVersion {
            path: "test.txt".into(),
            version: 7,
            sha256: "REMOTE".into(),
            device_id: "B".into(),
            event_id: "2".into(),
        };

        let result = store.resolve(local, remote).unwrap();

        assert_eq!(result, Resolution::Conflict);

        let content = std::fs::read_to_string(dir.path().join(".meshos/conflicts.jsonl")).unwrap();

        assert!(content.contains("test.txt"));
        assert!(content.contains("LOCAL"));
        assert!(content.contains("REMOTE"));
    }
}

pub mod conflicts;

#[cfg(test)]
mod conflict_copy_tests {
    use super::conflicts::conflict_copy_path;

    #[test]
    fn conflict_copy_name_is_deterministic() {
        let path = conflict_copy_path("/tmp/workspace", "docs/test.txt", "device-B", 7);

        assert_eq!(
            path.to_string_lossy(),
            "/tmp/workspace/docs/test.txt.conflict-device-B-v7"
        );
    }
}

pub mod apply;

#[cfg(test)]
mod apply_tests {
    use super::apply::{ApplyResult, apply_remote};
    use super::conflict::FileVersion;
    use tempfile::tempdir;

    #[test]
    fn newer_remote_replaces_local() {
        let dir = tempdir().unwrap();

        std::fs::create_dir_all(dir.path()).unwrap();

        let local = FileVersion {
            path: "test.txt".into(),
            version: 1,
            sha256: "local".into(),
            device_id: "A".into(),
            event_id: "1".into(),
        };

        let remote = FileVersion {
            path: "test.txt".into(),
            version: 2,
            sha256: "remote".into(),
            device_id: "B".into(),
            event_id: "2".into(),
        };

        let result = apply_remote(dir.path(), Some(&local), &remote, b"REMOTE DATA").unwrap();

        assert_eq!(result, ApplyResult::AppliedRemote);

        assert_eq!(
            std::fs::read_to_string(dir.path().join("test.txt")).unwrap(),
            "REMOTE DATA"
        );
    }

    #[test]
    fn older_remote_keeps_local() {
        let dir = tempdir().unwrap();

        let local = FileVersion {
            path: "test.txt".into(),
            version: 5,
            sha256: "local".into(),
            device_id: "A".into(),
            event_id: "1".into(),
        };

        let remote = FileVersion {
            path: "test.txt".into(),
            version: 3,
            sha256: "remote".into(),
            device_id: "B".into(),
            event_id: "2".into(),
        };

        let result = apply_remote(dir.path(), Some(&local), &remote, b"REMOTE DATA").unwrap();

        assert_eq!(result, ApplyResult::KeptLocal);
    }

    #[test]
    fn equal_version_different_content_creates_conflict() {
        let dir = tempdir().unwrap();

        let local = FileVersion {
            path: "test.txt".into(),
            version: 7,
            sha256: "local".into(),
            device_id: "A".into(),
            event_id: "1".into(),
        };

        let remote = FileVersion {
            path: "test.txt".into(),
            version: 7,
            sha256: "remote".into(),
            device_id: "B".into(),
            event_id: "2".into(),
        };

        let result = apply_remote(dir.path(), Some(&local), &remote, b"REMOTE DATA").unwrap();

        match result {
            ApplyResult::ConflictCreated(path) => {
                assert!(path.exists());
                assert!(std::fs::read_to_string(path).unwrap() == "REMOTE DATA");
            }

            other => panic!("unexpected result: {:?}", other),
        }
    }

    #[test]
    fn identical_content_is_not_rewritten() {
        let dir = tempdir().unwrap();

        let local = FileVersion {
            path: "test.txt".into(),
            version: 4,
            sha256: "same".into(),
            device_id: "A".into(),
            event_id: "1".into(),
        };

        let remote = FileVersion {
            path: "test.txt".into(),
            version: 9,
            sha256: "same".into(),
            device_id: "B".into(),
            event_id: "2".into(),
        };

        let result = apply_remote(dir.path(), Some(&local), &remote, b"REMOTE DATA").unwrap();

        assert_eq!(result, ApplyResult::AlreadyIdentical);
    }
}

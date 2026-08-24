use crate::conflict::FileVersion;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub next_version: u64,
    pub files: HashMap<String, FileVersion>,
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self {
            next_version: 1,
            files: HashMap::new(),
        }
    }
}

pub struct StateStore {
    path: PathBuf,
}

impl StateStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let dir = root.as_ref().join(".meshos");
        fs::create_dir_all(&dir)?;

        let store = Self {
            path: dir.join("state.json"),
        };

        if !store.path.exists() {
            store.save(&WorkspaceState::default())?;
        }

        Ok(store)
    }

    pub fn load(&self) -> Result<WorkspaceState, Box<dyn std::error::Error>> {
        let data = fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn save(&self, state: &WorkspaceState) -> Result<(), Box<dyn std::error::Error>> {
        let temp = self.path.with_extension("tmp");

        fs::write(&temp, serde_json::to_vec_pretty(state)?)?;

        fs::rename(temp, &self.path)?;
        Ok(())
    }

    pub fn upsert(
        &self,
        mut state: WorkspaceState,
        version: FileVersion,
    ) -> Result<WorkspaceState, Box<dyn std::error::Error>> {
        state.files.insert(version.path.clone(), version);

        state.next_version = state.next_version.saturating_add(1);

        self.save(&state)?;
        Ok(state)
    }
}

impl StateStore {
    pub fn next_version(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let mut state = self.load()?;
        let version = state.next_version;
        state.next_version = state.next_version.saturating_add(1);
        self.save(&state)?;
        Ok(version)
    }
}

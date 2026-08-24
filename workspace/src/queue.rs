use crate::protocol::SyncRecord;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingRecord {
    pub record: SyncRecord,
    pub attempts: u32,
    pub last_error: Option<String>,
}

pub struct SyncQueue {
    path: PathBuf,
}

impl SyncQueue {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, Box<dyn std::error::Error>> {
        let root = root.into();
        let dir = root.join(".meshos");

        fs::create_dir_all(&dir)?;

        Ok(Self {
            path: dir.join("pending.jsonl"),
        })
    }

    pub fn push(&self, record: SyncRecord) -> Result<(), Box<dyn std::error::Error>> {
        let pending = PendingRecord {
            record,
            attempts: 0,
            last_error: None,
        };

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;

        writeln!(file, "{}", serde_json::to_string(&pending)?)?;

        file.sync_all()?;

        Ok(())
    }

    pub fn load(&self) -> Result<Vec<PendingRecord>, Box<dyn std::error::Error>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = fs::File::open(&self.path)?;
        let reader = BufReader::new(file);

        let mut result = Vec::new();

        for line in reader.lines() {
            let line = line?;

            if line.trim().is_empty() {
                continue;
            }

            result.push(serde_json::from_str(&line)?);
        }

        Ok(result)
    }

    pub fn replace(&self, records: &[PendingRecord]) -> Result<(), Box<dyn std::error::Error>> {
        let temp = self.path.with_extension("tmp");

        {
            let mut file = fs::File::create(&temp)?;

            for record in records {
                writeln!(file, "{}", serde_json::to_string(record)?)?;
            }

            file.sync_all()?;
        }

        fs::rename(temp, &self.path)?;

        Ok(())
    }
}

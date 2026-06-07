use orally_core::OrallyError;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub created_at_ms: u128,
    pub raw_text: String,
    pub final_text: String,
    pub provider: String,
}

impl HistoryEntry {
    pub fn new(
        raw_text: impl Into<String>,
        final_text: impl Into<String>,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            created_at_ms: now_ms(),
            raw_text: raw_text.into(),
            final_text: final_text.into(),
            provider: provider.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn append(&self, entry: &HistoryEntry) -> Result<(), OrallyError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| OrallyError::Processing(error.to_string()))?;
        }

        let line = serde_json::to_string(entry)
            .map_err(|error| OrallyError::Processing(error.to_string()))?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|error| OrallyError::Processing(error.to_string()))?;

        writeln!(file, "{line}").map_err(|error| OrallyError::Processing(error.to_string()))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn default_history_path(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .map(|parent| parent.join("history.jsonl"))
        .unwrap_or_else(|| PathBuf::from("history.jsonl"))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_history_path_from_config_path() {
        let path =
            default_history_path(Path::new("C:/Users/me/AppData/Roaming/Orally/config.toml"));

        assert!(path.ends_with("history.jsonl"));
    }
}

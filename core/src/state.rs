//! Hitch's own memory between launches. Never holds the API key.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::claude::DeploymentMode;
use crate::gateway::Model;
use crate::Result;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HitchState {
    /// The profile Hitch created in Claude's config library.
    pub profile_id: Option<String>,
    /// What Claude had applied before Hitch, so Remove can put it back.
    pub previous_applied_id: Option<String>,
    pub previous_mode: Option<DeploymentMode>,
    pub base_url: Option<String>,
    /// The models the user chose to show in Claude, default first. Empty means all of them.
    pub models: Vec<String>,
    /// The chat models Oxen offered when Hitch last asked. Kept so Claude's model list can
    /// be rewritten without the network, for example when switching Oxen back on.
    pub catalog: Vec<Model>,
    /// The last few characters of the key, so the user can tell which one is in use.
    pub key_hint: Option<String>,
    /// When Hitch last changed Claude's config (unix seconds). A Claude that started
    /// before this has not picked the change up yet.
    pub last_write_at: Option<u64>,
}

pub struct StateStore {
    path: PathBuf,
}

impl StateStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Missing or unreadable state reads as a fresh install.
    pub fn load(&self) -> HitchState {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, state: &HitchState) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&self.path, serde_json::to_string_pretty(state)?)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trips_and_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path().join("nested/state.json"));
        assert_eq!(store.load(), HitchState::default());

        let state = HitchState {
            profile_id: Some("abc".into()),
            previous_mode: Some(DeploymentMode::Anthropic),
            models: vec!["m".into()],
            ..Default::default()
        };
        store.save(&state).unwrap();
        assert_eq!(store.load(), state);
    }

    #[test]
    fn corrupt_state_reads_as_a_fresh_install() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        fs::write(&path, "nope").unwrap();
        assert_eq!(StateStore::new(path).load(), HitchState::default());
    }
}

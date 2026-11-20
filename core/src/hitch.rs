//! What Hitch does, as the UI sees it: connect Claude to Oxen, pause, change models, remove.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::claude::library::{self, GatewayProfile};
use crate::claude::{ClaudeApp, ClaudePaths, DeploymentMode};
use crate::gateway::{self, DEFAULT_BASE_URL};
use crate::secrets::SecretStore;
use crate::state::{HitchState, StateStore};
use crate::{Error, Result};

pub struct Hitch {
    pub paths: ClaudePaths,
    pub state: StateStore,
    pub secrets: Box<dyn SecretStore>,
    pub claude: Box<dyn ClaudeApp>,
}

/// How Claude Desktop relates to Oxen right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Link {
    /// Hitch has not set Claude up yet.
    NotConfigured,
    /// Claude is set to run on Oxen.
    Connected,
    /// Set up, but Claude is set to sign in to Anthropic instead.
    Paused,
    /// Hitch set Claude up, but Claude's config has since been changed by something else.
    Drifted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub claude_installed: bool,
    pub claude_running: bool,
    /// An administrator manages Claude's inference settings, so Hitch's are ignored.
    pub managed: bool,
    pub link: Link,
    /// Claude is running with config older than Hitch's last change.
    pub needs_restart: bool,
    pub base_url: String,
    pub key_hint: Option<String>,
    pub models: Vec<String>,
}

impl Hitch {
    pub fn status(&self) -> Status {
        let state = self.state.load();
        let started_at = self.claude.started_at();
        Status {
            claude_installed: self.claude.installed(),
            claude_running: started_at.is_some(),
            managed: self.paths.managed(),
            link: self.link(&state),
            needs_restart: matches!(
                (started_at, state.last_write_at),
                (Some(started), Some(written)) if started <= written
            ),
            base_url: self.base_url(&state),
            key_hint: state.key_hint,
            models: state.models,
        }
    }

    /// The stored key, if there is one and the keychain lets us read it.
    pub fn api_key(&self) -> Option<String> {
        self.secrets.get().ok().flatten()
    }

    /// Points Claude at the gateway. Pass a key the first time, or to replace it;
    /// pass a base URL to change it. Anything omitted keeps its current value.
    pub fn connect(&self, api_key: Option<String>, base_url: Option<String>) -> Result<Status> {
        let mut state = self.state.load();
        let new_key = api_key
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty());
        let base_url = match base_url {
            Some(url) => gateway::normalize_base_url(&url)?,
            None => self.base_url(&state),
        };

        let profile_id = match &state.profile_id {
            Some(id) => id.clone(),
            None => {
                // First time: remember what Claude had, so Remove can put it back.
                let before = library::snapshot(&self.paths);
                state.previous_applied_id = before.applied_id;
                state.previous_mode = before.mode;
                library::new_profile_id()
            }
        };

        library::apply_profile(
            &self.paths,
            &profile_id,
            &GatewayProfile {
                base_url: base_url.clone(),
                // Without a new key, rewrite the one we hold; failing that, keep Claude's.
                api_key: new_key.clone().or_else(|| self.api_key()),
                models: state.models.clone(),
            },
        )?;

        state.profile_id = Some(profile_id);
        state.base_url = Some(base_url);
        state.last_write_at = Some(now());
        if let Some(key) = &new_key {
            state.key_hint = Some(hint(key));
        }
        self.state.save(&state)?;

        if let Some(key) = &new_key {
            self.secrets.set(key)?;
        }
        Ok(self.status())
    }

    /// Chooses which models Claude shows, default first. Empty lets Claude discover them.
    pub fn set_models(&self, models: Vec<String>) -> Result<Status> {
        let mut state = self.configured()?;
        state.models = models;
        self.state.save(&state)?;
        self.connect(None, None)
    }

    /// On: Claude runs on Oxen. Off: Claude goes back to signing in with Anthropic.
    /// The profile and key stay in place either way.
    pub fn set_routing(&self, enabled: bool) -> Result<Status> {
        let mut state = self.configured()?;
        if enabled {
            // Re-applying also repairs config that drifted.
            return self.connect(None, None);
        }
        library::set_mode(&self.paths, Some(DeploymentMode::Anthropic))?;
        state.last_write_at = Some(now());
        self.state.save(&state)?;
        Ok(self.status())
    }

    /// Takes Hitch's profile out of Claude, restores what was applied before, and
    /// forgets the key.
    pub fn remove(&self) -> Result<Status> {
        let state = self.state.load();
        if let Some(id) = &state.profile_id {
            library::remove_profile(
                &self.paths,
                id,
                state.previous_applied_id.as_deref(),
                state.previous_mode,
            )?;
        }
        self.state.save(&HitchState {
            last_write_at: Some(now()),
            ..Default::default()
        })?;
        self.secrets.delete()?;
        Ok(self.status())
    }

    pub fn restart_claude(&self) -> Result<Status> {
        self.claude.restart()?;
        Ok(self.status())
    }

    fn configured(&self) -> Result<HitchState> {
        let state = self.state.load();
        match state.profile_id {
            Some(_) => Ok(state),
            None => Err(Error::invalid("Connect to Oxen first.")),
        }
    }

    fn link(&self, state: &HitchState) -> Link {
        let Some(id) = &state.profile_id else {
            return Link::NotConfigured;
        };
        let on_disk = library::snapshot(&self.paths);
        let ours_is_applied = on_disk.applied_id.as_ref() == Some(id)
            && on_disk
                .applied
                .is_some_and(|p| p.provider.as_deref() == Some("gateway") && p.has_key);
        match (ours_is_applied, on_disk.mode) {
            (false, _) => Link::Drifted,
            (true, Some(DeploymentMode::Anthropic)) => Link::Paused,
            (true, _) => Link::Connected,
        }
    }

    fn base_url(&self, state: &HitchState) -> String {
        state
            .base_url
            .clone()
            .or_else(|| std::env::var("HITCH_BASE_URL").ok())
            .unwrap_or_else(|| DEFAULT_BASE_URL.into())
    }
}

/// The last four characters, enough to recognize a key without revealing it.
fn hint(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    chars[chars.len().saturating_sub(4)..].iter().collect()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::process::fake::FakeClaude;
    use crate::secrets::MemorySecrets;
    use serde_json::{json, Value};
    use std::fs;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    const OTHER: &str = "c2898676-28c3-4ef2-90bb-6a10501a33c1";

    struct Shared(Arc<FakeClaude>);
    impl ClaudeApp for Shared {
        fn installed(&self) -> bool {
            self.0.installed()
        }
        fn started_at(&self) -> Option<u64> {
            self.0.started_at()
        }
        fn restart(&self) -> Result<()> {
            self.0.restart()
        }
    }

    struct World {
        _dir: tempfile::TempDir,
        hitch: Hitch,
        claude: Arc<FakeClaude>,
    }

    fn world(claude: FakeClaude) -> World {
        let dir = tempfile::tempdir().unwrap();
        let claude = Arc::new(claude);
        let hitch = Hitch {
            paths: ClaudePaths::at(dir.path().join("Claude-3p")),
            state: StateStore::new(dir.path().join("hitch/state.json")),
            secrets: Box::new(MemorySecrets::default()),
            claude: Box::new(Shared(claude.clone())),
        };
        World {
            _dir: dir,
            hitch,
            claude,
        }
    }

    fn read(path: std::path::PathBuf) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    fn applied_profile(hitch: &Hitch) -> Value {
        let id = hitch.state.load().profile_id.unwrap();
        read(hitch.paths.profile_file(&id))
    }

    #[test]
    fn before_setup_nothing_is_configured_and_the_default_gateway_is_offered() {
        let w = world(FakeClaude::not_running());
        let status = w.hitch.status();
        assert_eq!(status.link, Link::NotConfigured);
        assert_eq!(status.base_url, "https://hub.oxen.ai/api/ai");
        assert!(status.claude_installed);
        assert!(!status.claude_running);
        assert!(!status.needs_restart);
        assert!(!w.hitch.paths.data_dir.exists(), "status must not write anything");
    }

    #[test]
    fn connecting_with_a_key_sets_claude_up_and_remembers_the_key() {
        let w = world(FakeClaude::not_running());

        let status = w.hitch.connect(Some("  oxen_secret_1234 \n".into()), None).unwrap();

        assert_eq!(status.link, Link::Connected);
        assert_eq!(status.key_hint.as_deref(), Some("1234"));
        assert_eq!(w.hitch.api_key().as_deref(), Some("oxen_secret_1234"));
        let profile = applied_profile(&w.hitch);
        assert_eq!(profile["inferenceGatewayApiKey"], "oxen_secret_1234");
        assert_eq!(profile["inferenceGatewayBaseUrl"], "https://hub.oxen.ai/api/ai");
        assert_eq!(read(w.hitch.paths.desktop_config())["deploymentMode"], "3p");
    }

    #[test]
    fn the_key_never_lands_in_hitchs_state_file() {
        let w = world(FakeClaude::not_running());
        w.hitch.connect(Some("oxen_secret_1234".into()), None).unwrap();
        let state = fs::read_to_string(w._dir.path().join("hitch/state.json")).unwrap();
        assert!(!state.contains("oxen_secret"));
    }

    #[test]
    fn connecting_without_a_key_is_refused_and_changes_nothing() {
        let w = world(FakeClaude::not_running());
        assert!(w.hitch.connect(Some("   ".into()), None).is_err());
        assert_eq!(w.hitch.status().link, Link::NotConfigured);
        assert!(!w.hitch.paths.data_dir.exists());
    }

    #[test]
    fn a_bad_gateway_address_is_refused() {
        let w = world(FakeClaude::not_running());
        assert!(w.hitch.connect(Some("k".into()), Some("hub.oxen.ai".into())).is_err());
        assert_eq!(w.hitch.status().link, Link::NotConfigured);
    }

    #[test]
    fn the_gateway_address_can_change_without_retyping_the_key() {
        let w = world(FakeClaude::not_running());
        w.hitch.connect(Some("key-abcd".into()), None).unwrap();

        let status = w
            .hitch
            .connect(None, Some("http://localhost:3001/api/ai/v1/".into()))
            .unwrap();

        assert_eq!(status.base_url, "http://localhost:3001/api/ai");
        let profile = applied_profile(&w.hitch);
        assert_eq!(profile["inferenceGatewayBaseUrl"], "http://localhost:3001/api/ai");
        assert_eq!(profile["inferenceGatewayApiKey"], "key-abcd");
    }

    #[test]
    fn a_running_claude_needs_a_restart_until_it_gets_one() {
        let w = world(FakeClaude::running_since(1));
        assert!(!w.hitch.status().needs_restart, "nothing has changed yet");

        assert!(w.hitch.connect(Some("k".into()), None).unwrap().needs_restart);

        let status = w.hitch.restart_claude().unwrap();
        assert!(status.claude_running);
        assert!(!status.needs_restart);
    }

    #[test]
    fn a_claude_that_is_not_running_never_needs_a_restart() {
        let w = world(FakeClaude::not_running());
        assert!(!w.hitch.connect(Some("k".into()), None).unwrap().needs_restart);
    }

    #[test]
    fn switching_back_to_anthropic_and_on_again_keeps_the_setup() {
        let w = world(FakeClaude::not_running());
        w.hitch.connect(Some("k".into()), None).unwrap();
        let profile_before = applied_profile(&w.hitch);

        let off = w.hitch.set_routing(false).unwrap();
        assert_eq!(off.link, Link::Paused);
        assert_eq!(read(w.hitch.paths.desktop_config())["deploymentMode"], "1p");
        assert_eq!(applied_profile(&w.hitch), profile_before);

        let on = w.hitch.set_routing(true).unwrap();
        assert_eq!(on.link, Link::Connected);
        assert_eq!(read(w.hitch.paths.desktop_config())["deploymentMode"], "3p");
    }

    #[test]
    fn chosen_models_are_written_default_first() {
        let w = world(FakeClaude::not_running());
        w.hitch.connect(Some("k".into()), None).unwrap();

        let status = w.hitch.set_models(vec!["opus".into(), "llama".into()]).unwrap();

        assert_eq!(status.models, vec!["opus", "llama"]);
        assert_eq!(applied_profile(&w.hitch)["inferenceModels"], json!(["opus", "llama"]));

        w.hitch.set_models(vec![]).unwrap();
        assert!(applied_profile(&w.hitch).get("inferenceModels").is_none());
    }

    #[test]
    fn settings_cannot_change_before_setup() {
        let w = world(FakeClaude::not_running());
        assert!(w.hitch.set_models(vec!["m".into()]).is_err());
        assert!(w.hitch.set_routing(false).is_err());
    }

    #[test]
    fn config_changed_behind_hitchs_back_is_reported_and_reconnect_repairs_it() {
        let w = world(FakeClaude::not_running());
        w.hitch.connect(Some("k".into()), None).unwrap();
        let ours = w.hitch.state.load().profile_id.unwrap();
        // Someone applies a different configuration inside Claude.
        fs::write(
            w.hitch.paths.meta_file(),
            json!({"appliedId": OTHER, "entries": [{"id": ours, "name": "Oxen"}, {"id": OTHER, "name": "Mine"}]})
                .to_string(),
        )
        .unwrap();

        assert_eq!(w.hitch.status().link, Link::Drifted);

        assert_eq!(w.hitch.set_routing(true).unwrap().link, Link::Connected);
        let meta = read(w.hitch.paths.meta_file());
        assert_eq!(meta["appliedId"], json!(ours));
        assert_eq!(meta["entries"].as_array().unwrap().len(), 2, "their entry stays");
    }

    #[test]
    fn remove_puts_back_what_claude_had_before_and_forgets_the_key() {
        let w = world(FakeClaude::not_running());
        let paths = &w.hitch.paths;
        fs::create_dir_all(paths.library_dir()).unwrap();
        let meta_before = json!({"appliedId": OTHER, "entries": [{"id": OTHER, "name": "Default"}]});
        fs::write(paths.meta_file(), meta_before.to_string()).unwrap();
        fs::write(paths.profile_file(OTHER), json!({"inferenceProvider": "bedrock"}).to_string())
            .unwrap();
        fs::write(paths.desktop_config(), json!({"deploymentMode": "1p"}).to_string()).unwrap();

        w.hitch.connect(Some("k".into()), None).unwrap();
        let status = w.hitch.remove().unwrap();

        assert_eq!(status.link, Link::NotConfigured);
        assert_eq!(status.key_hint, None);
        assert_eq!(w.hitch.api_key(), None);
        assert_eq!(read(paths.meta_file()), meta_before);
        assert_eq!(read(paths.desktop_config()), json!({"deploymentMode": "1p"}));
        assert_eq!(fs::read_dir(paths.library_dir()).unwrap().count(), 2);
    }

    #[test]
    fn removing_while_claude_runs_still_asks_for_a_restart() {
        let w = world(FakeClaude::running_since(1));
        w.hitch.connect(Some("k".into()), None).unwrap();
        w.claude.started_at.store(2, Ordering::SeqCst);
        assert!(w.hitch.remove().unwrap().needs_restart);
    }
}

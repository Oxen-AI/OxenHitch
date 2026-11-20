//! What Hitch does, as the UI sees it: connect Claude to Oxen, pause, change models, remove.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::claude::library::{self, GatewayProfile};
use crate::claude::models;
use crate::claude::{ClaudeApp, ClaudePaths, DeploymentMode};
use crate::gateway::{self, Model, DEFAULT_BASE_URL};
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
            key_hint: self.key_for(&state).as_deref().map(hint),
            base_url: self.base_url(&state),
            models: state.models,
        }
    }

    /// The saved key for the host Hitch is pointed at, if there is one.
    pub fn api_key(&self) -> Option<String> {
        self.key_for(&self.state.load())
    }

    /// A hint of the key already saved for a host (as typed in the Host field), so the UI
    /// can say that nothing needs pasting.
    pub fn saved_key_hint(&self, host: &str) -> Option<String> {
        let base_url = gateway::normalize_base_url(host).ok()?;
        let key = self.secrets.get(host_of(&base_url)).ok().flatten()?;
        Some(hint(&key))
    }

    fn key_for(&self, state: &HitchState) -> Option<String> {
        let base_url = self.base_url(state);
        self.secrets.get(host_of(&base_url)).ok().flatten()
    }

    /// Points Claude at the gateway. Pass a key to use a new one; pass a base URL to
    /// change hosts. Without a key, the one already saved for the host is used, which
    /// includes a key the Oxen CLI saved. Moving to a host with no saved key carries the
    /// current key over.
    pub fn connect(&self, api_key: Option<String>, base_url: Option<String>) -> Result<Status> {
        let mut state = self.state.load();
        let new_key = api_key
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty());
        let base_url = match base_url {
            Some(url) => gateway::normalize_base_url(&url)?,
            None => self.base_url(&state),
        };

        let host = host_of(&base_url).to_string();
        let saved_for_host = self.secrets.get(&host).ok().flatten();
        let key = new_key
            .or_else(|| saved_for_host.clone())
            .or_else(|| self.key_for(&state));

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
                // With no key at all, the profile keeps the one Claude already has.
                api_key: key.clone(),
                models: models::entries_for(&state.models, &state.catalog),
            },
        )?;

        state.profile_id = Some(profile_id);
        state.base_url = Some(base_url);
        state.last_write_at = Some(now());
        self.state.save(&state)?;

        if let Some(key) = key.filter(|key| saved_for_host.as_ref() != Some(key)) {
            self.secrets.set(&host, &key)?;
        }
        Ok(self.status())
    }

    /// Records the chat models Oxen offers right now. Nothing is written to Claude: the
    /// list is used the next time Hitch applies its profile (connect, model choice,
    /// switching on).
    pub fn remember_catalog(&self, catalog: Vec<Model>) -> Result<()> {
        let mut state = self.state.load();
        if state.catalog != catalog {
            state.catalog = catalog;
            self.state.save(&state)?;
        }
        Ok(())
    }

    /// Chooses which models Claude shows, default first. Empty shows every model Oxen offers.
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

    /// Takes Hitch's profile out of Claude and restores what was applied before. The key
    /// stays in the Oxen settings file, because the Oxen CLI uses it too.
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

/// `hub.oxen.ai` from `https://hub.oxen.ai/api/ai`: the name keys are saved under.
fn host_of(base_url: &str) -> &str {
    let rest = base_url.split_once("://").map_or(base_url, |(_, rest)| rest);
    rest.split('/').next().unwrap_or(rest)
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
    fn a_host_alone_is_enough_to_change_gateways() {
        let w = world(FakeClaude::not_running());
        let status = w.hitch.connect(Some("k".into()), Some("localhost:3001".into())).unwrap();
        assert_eq!(status.base_url, "http://localhost:3001/api/ai");
        assert_eq!(
            applied_profile(&w.hitch)["inferenceGatewayBaseUrl"],
            "http://localhost:3001/api/ai"
        );
    }

    #[test]
    fn a_bad_gateway_address_is_refused() {
        let w = world(FakeClaude::not_running());
        assert!(w.hitch.connect(Some("k".into()), Some("not a host".into())).is_err());
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
    fn a_key_the_oxen_cli_saved_is_used_without_pasting() {
        let w = world(FakeClaude::not_running());
        w.hitch.secrets.set("hub.oxen.ai", "cli-token-9876").unwrap();

        assert_eq!(w.hitch.status().key_hint.as_deref(), Some("9876"));
        let status = w.hitch.connect(None, None).unwrap();

        assert_eq!(status.link, Link::Connected);
        assert_eq!(applied_profile(&w.hitch)["inferenceGatewayApiKey"], "cli-token-9876");
    }

    #[test]
    fn each_host_uses_its_own_saved_key() {
        let w = world(FakeClaude::not_running());
        w.hitch.secrets.set("hub.oxen.ai", "hub-token").unwrap();
        w.hitch.secrets.set("localhost:3001", "local-token").unwrap();
        w.hitch.connect(None, None).unwrap();

        let status = w.hitch.connect(None, Some("localhost:3001".into())).unwrap();

        assert_eq!(applied_profile(&w.hitch)["inferenceGatewayApiKey"], "local-token");
        assert_eq!(status.key_hint.as_deref(), Some("oken"));
        assert_eq!(w.hitch.api_key().as_deref(), Some("local-token"));
        assert_eq!(w.hitch.secrets.get("hub.oxen.ai").unwrap().as_deref(), Some("hub-token"));
    }

    #[test]
    fn a_pasted_key_is_saved_for_its_host_only() {
        let w = world(FakeClaude::not_running());
        w.hitch.secrets.set("hub.oxen.ai", "hub-token").unwrap();

        w.hitch.connect(Some("pasted".into()), Some("dev.hub.oxen.ai".into())).unwrap();

        assert_eq!(w.hitch.secrets.get("dev.hub.oxen.ai").unwrap().as_deref(), Some("pasted"));
        assert_eq!(w.hitch.secrets.get("hub.oxen.ai").unwrap().as_deref(), Some("hub-token"));
    }

    #[test]
    fn moving_to_a_host_with_no_saved_key_brings_the_current_key_along() {
        let w = world(FakeClaude::not_running());
        w.hitch.connect(Some("my-key".into()), None).unwrap();

        w.hitch.connect(None, Some("localhost:3001".into())).unwrap();

        assert_eq!(w.hitch.api_key().as_deref(), Some("my-key"));
        assert_eq!(w.hitch.secrets.get("localhost:3001").unwrap().as_deref(), Some("my-key"));
    }

    #[test]
    fn a_saved_key_can_be_looked_up_by_whatever_was_typed_as_the_host() {
        let w = world(FakeClaude::not_running());
        w.hitch.secrets.set("localhost:3001", "local-token-4321").unwrap();
        assert_eq!(w.hitch.saved_key_hint(" localhost:3001 ").as_deref(), Some("4321"));
        assert_eq!(w.hitch.saved_key_hint("http://localhost:3001/api/ai").as_deref(), Some("4321"));
        assert_eq!(w.hitch.saved_key_hint("hub.oxen.ai"), None);
        assert_eq!(w.hitch.saved_key_hint("not a host"), None);
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

    fn oxen_catalog() -> Vec<Model> {
        let model = |id: &str, name: &str, developer: &str, uuid: Option<&str>| Model {
            id: id.into(),
            name: Some(name.into()),
            developer: Some(developer.into()),
            uuid: uuid.map(String::from),
        };
        vec![
            model("gpt-5-mini", "GPT 5 Mini", "openai", Some("0b9f6c1e-7a52-4d3e-9f10-2c5a8e4b7d61")),
            model("claude-opus-5-5", "Claude Opus 5.5", "anthropic", None),
        ]
    }

    #[test]
    fn connecting_lists_every_oxen_model_in_claude() {
        let w = world(FakeClaude::not_running());
        w.hitch.remember_catalog(oxen_catalog()).unwrap();

        w.hitch.connect(Some("k".into()), None).unwrap();

        assert_eq!(
            applied_profile(&w.hitch)["inferenceModels"],
            json!([
                "claude-opus-5-5",
                {
                    "name": "claude-via-0b9f6c1e-7a52-4d3e-9f10-2c5a8e4b7d61",
                    "labelOverride": "GPT 5 Mini",
                    "behavesAs": "claude-sonnet-5"
                }
            ])
        );
    }

    #[test]
    fn the_model_list_survives_switching_off_and_on_without_the_network() {
        let w = world(FakeClaude::not_running());
        w.hitch.remember_catalog(oxen_catalog()).unwrap();
        w.hitch.connect(Some("k".into()), None).unwrap();
        let before = applied_profile(&w.hitch)["inferenceModels"].clone();

        w.hitch.set_routing(false).unwrap();
        w.hitch.set_routing(true).unwrap();

        assert_eq!(applied_profile(&w.hitch)["inferenceModels"], before);
    }

    #[test]
    fn choosing_models_narrows_the_list_and_sets_the_default() {
        let w = world(FakeClaude::not_running());
        w.hitch.remember_catalog(oxen_catalog()).unwrap();
        w.hitch.connect(Some("k".into()), None).unwrap();

        w.hitch.set_models(vec!["gpt-5-mini".into()]).unwrap();

        let models = applied_profile(&w.hitch)["inferenceModels"].clone();
        assert_eq!(models.as_array().unwrap().len(), 1);
        assert_eq!(models[0]["labelOverride"], "GPT 5 Mini");
    }

    #[test]
    fn a_newer_catalog_only_reaches_claude_on_the_next_change() {
        let w = world(FakeClaude::running_since(1));
        w.hitch.connect(Some("k".into()), None).unwrap();
        w.hitch.restart_claude().unwrap();

        w.hitch.remember_catalog(oxen_catalog()).unwrap();

        assert!(applied_profile(&w.hitch).get("inferenceModels").is_none());
        assert!(!w.hitch.status().needs_restart, "remembering must not touch Claude");
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
    fn remove_puts_back_what_claude_had_before_and_leaves_the_key_alone() {
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
        assert_eq!(w.hitch.api_key().as_deref(), Some("k"), "the Oxen CLI still needs its key");
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

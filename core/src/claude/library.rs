//! Reads and writes Claude Desktop's `configLibrary/` and its deployment mode.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::models::ModelEntry;
use super::paths::ClaudePaths;
use crate::{Error, Result};

/// The name Hitch gives its entry in Claude's list of saved configurations.
pub const PROFILE_NAME: &str = "Oxen";

/// The user's choice between signing in to Anthropic and using third-party inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeploymentMode {
    #[serde(rename = "1p")]
    Anthropic,
    #[serde(rename = "3p")]
    ThirdParty,
}

/// What Hitch wants Claude to use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayProfile {
    pub base_url: String,
    /// `None` keeps whatever key the profile already has (it may be sealed by Claude).
    pub api_key: Option<String>,
    /// Models to show, default first. Empty lets Claude discover them.
    pub models: Vec<ModelEntry>,
}

/// What a saved configuration currently says, as far as Hitch cares.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileView {
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub has_key: bool,
    pub models: Vec<String>,
}

/// Claude's third-party config as it is on disk right now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub applied_id: Option<String>,
    pub entry_ids: Vec<String>,
    pub mode: Option<DeploymentMode>,
    pub applied: Option<ProfileView>,
}

pub fn new_profile_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Never fails: anything missing or unreadable reads as absent, the way Claude treats it.
pub fn snapshot(paths: &ClaudePaths) -> Snapshot {
    let meta = read_object(&paths.meta_file()).unwrap_or_default();
    let applied_id = meta
        .get(APPLIED_ID)
        .and_then(Value::as_str)
        .filter(|id| is_profile_id(id))
        .map(String::from);
    let mode = read_object(&paths.desktop_config())
        .and_then(|config| config.get(DEPLOYMENT_MODE).cloned())
        .and_then(|value| serde_json::from_value(value).ok());
    let applied = applied_id
        .as_deref()
        .and_then(|id| read_object(&paths.profile_file(id)))
        .map(|profile| view(&profile));

    Snapshot {
        applied_id,
        entry_ids: entry_ids(&meta),
        mode,
        applied,
    }
}

/// Writes the profile, makes it the applied one, and puts Claude in third-party mode.
pub fn apply_profile(paths: &ClaudePaths, id: &str, profile: &GatewayProfile) -> Result<()> {
    let file = paths.profile_file(id);
    let mut body = read_object(&file).unwrap_or_default();

    let has_existing_key = body
        .get(API_KEY)
        .and_then(Value::as_str)
        .is_some_and(|key| !key.is_empty());
    if profile.api_key.is_none() && !has_existing_key {
        return Err(Error::invalid("An API key is needed to connect."));
    }
    // Read everything we are about to change before writing anything, so a file we
    // cannot parse stops us with nothing half-done.
    let mut desktop = read_object_strict(&paths.desktop_config())?;
    let mut meta = read_object(&paths.meta_file()).unwrap_or_default();

    match body.get_mut(CLAUDE_AI_IMPORT).and_then(Value::as_object_mut) {
        Some(import) => {
            import.insert("enabled".into(), json!(true));
        }
        None => {
            body.insert(CLAUDE_AI_IMPORT.into(), json!({ "enabled": true }));
        }
    }
    body.insert("inferenceProvider".into(), json!("gateway"));
    body.insert(BASE_URL.into(), json!(profile.base_url));
    if let Some(key) = &profile.api_key {
        body.insert(API_KEY.into(), json!(key));
    }
    body.insert("inferenceCredentialKind".into(), json!("static"));
    if profile.models.is_empty() {
        body.shift_remove(MODELS);
    } else {
        let models: Vec<Value> = profile.models.iter().map(ModelEntry::to_config).collect();
        body.insert(MODELS.into(), Value::Array(models));
    }

    let mut entries = entries(&meta);
    if !entries.iter().any(|entry| entry_id(entry) == Some(id)) {
        entries.push(json!({ "id": id, "name": PROFILE_NAME }));
    }
    meta.insert(APPLIED_ID.into(), json!(id));
    meta.insert(ENTRIES.into(), Value::Array(entries));

    desktop.insert(
        DEPLOYMENT_MODE.into(),
        serde_json::to_value(DeploymentMode::ThirdParty)?,
    );

    write_object(&file, &body)?;
    write_object(&paths.meta_file(), &meta)?;
    write_object(&paths.desktop_config(), &desktop)
}

/// `None` clears the choice, which leaves Claude to decide from the applied profile.
pub fn set_mode(paths: &ClaudePaths, mode: Option<DeploymentMode>) -> Result<()> {
    let file = paths.desktop_config();
    let mut desktop = read_object_strict(&file)?;
    match mode {
        Some(mode) => {
            desktop.insert(DEPLOYMENT_MODE.into(), serde_json::to_value(mode)?);
        }
        None if desktop.shift_remove(DEPLOYMENT_MODE).is_none() => return Ok(()),
        None => {}
    }
    write_object(&file, &desktop)
}

/// Deletes the profile and re-applies `restore_applied` if it still exists.
pub fn remove_profile(
    paths: &ClaudePaths,
    id: &str,
    restore_applied: Option<&str>,
    restore_mode: Option<DeploymentMode>,
) -> Result<()> {
    // Checked first so an unparseable desktop config stops us before anything is deleted.
    read_object_strict(&paths.desktop_config())?;

    remove_if_present(&paths.profile_file(id))?;

    if let Some(mut meta) = read_object(&paths.meta_file()) {
        let mut entries = entries(&meta);
        entries.retain(|entry| entry_id(entry) != Some(id));
        let remaining: Vec<&str> = entries.iter().filter_map(entry_id).collect();

        if remaining.is_empty() {
            remove_if_present(&paths.meta_file())?;
            // Only succeeds when empty, which is what we want.
            let _ = fs::remove_dir(paths.library_dir());
        } else {
            if meta.get(APPLIED_ID).and_then(Value::as_str) == Some(id) {
                let next = restore_applied
                    .filter(|wanted| remaining.contains(wanted))
                    .unwrap_or(remaining[0]);
                meta.insert(APPLIED_ID.into(), json!(next));
            }
            meta.insert(ENTRIES.into(), Value::Array(entries));
            write_object(&paths.meta_file(), &meta)?;
        }
    }

    set_mode(paths, restore_mode)
}

const APPLIED_ID: &str = "appliedId";
const ENTRIES: &str = "entries";
const DEPLOYMENT_MODE: &str = "deploymentMode";
const CLAUDE_AI_IMPORT: &str = "claudeAiImport";
const BASE_URL: &str = "inferenceGatewayBaseUrl";
const API_KEY: &str = "inferenceGatewayApiKey";
const MODELS: &str = "inferenceModels";

type Object = Map<String, Value>;

/// Claude ignores an applied ID that does not look like this.
fn is_profile_id(id: &str) -> bool {
    id.len() == 36 && id.chars().all(|c| matches!(c, 'a'..='f' | '0'..='9' | '-'))
}

fn entries(meta: &Object) -> Vec<Value> {
    meta.get(ENTRIES)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn entry_id(entry: &Value) -> Option<&str> {
    entry.get("id").and_then(Value::as_str)
}

fn entry_ids(meta: &Object) -> Vec<String> {
    entries(meta)
        .iter()
        .filter_map(entry_id)
        .map(String::from)
        .collect()
}

fn view(profile: &Object) -> ProfileView {
    let text = |key: &str| profile.get(key).and_then(Value::as_str).map(String::from);
    ProfileView {
        provider: text("inferenceProvider"),
        base_url: text(BASE_URL),
        has_key: text(API_KEY).is_some_and(|key| !key.is_empty()),
        models: profile
            .get(MODELS)
            .and_then(Value::as_array)
            .map(|models| {
                models
                    .iter()
                    // An entry is a model ID, or an object naming one.
                    .filter_map(|m| m.as_str().or_else(|| m.get("name").and_then(Value::as_str)))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Missing or unparseable reads as `None`.
fn read_object(path: &Path) -> Option<Object> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// Missing reads as empty; a file that exists but is not a JSON object is an error,
/// because writing over it would destroy settings we do not understand.
fn read_object_strict(path: &Path) -> Result<Object> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|_| {
            Error::invalid(format!(
                "Claude's settings file could not be read, so nothing was changed: {}",
                path.display()
            ))
        }),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(Object::new()),
        Err(err) => Err(err.into()),
    }
}

/// Atomic: Claude never sees a half-written file. Private: these files can hold the key.
fn write_object(path: &Path, object: &Object) -> Result<()> {
    let dir = path.parent().expect("config files live in a directory");
    create_private_dir(dir)?;

    let mut text = serde_json::to_string_pretty(object)?;
    text.push('\n');

    let temp = dir.join(format!(".hitch-{}.tmp", uuid::Uuid::new_v4()));
    let written = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        fs::rename(&temp, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    Ok(written?)
}

/// Creates missing directories as owner-only. Existing ones are left as they are.
fn create_private_dir(dir: &Path) -> Result<()> {
    if dir.exists() {
        return Ok(());
    }
    if let Some(parent) = dir.parent() {
        create_private_dir(parent)?;
    }
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(dir) {
        Err(err) if err.kind() != ErrorKind::AlreadyExists => Err(err.into()),
        _ => Ok(()),
    }
}

fn remove_if_present(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(err) if err.kind() != ErrorKind::NotFound => Err(err.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::fs;

    const OTHER: &str = "c2898676-28c3-4ef2-90bb-6a10501a33c1";

    fn sandbox() -> (tempfile::TempDir, ClaudePaths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = ClaudePaths::at(dir.path().join("Claude-3p"));
        (dir, paths)
    }

    fn read(path: std::path::PathBuf) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    fn oxen(key: Option<&str>) -> GatewayProfile {
        GatewayProfile {
            base_url: "https://hub.oxen.ai/api/ai".into(),
            api_key: key.map(String::from),
            models: vec![],
        }
    }

    /// A machine where the user already set up some other configuration by hand.
    fn with_existing_profile(paths: &ClaudePaths) {
        fs::create_dir_all(paths.library_dir()).unwrap();
        fs::write(
            paths.meta_file(),
            json!({"appliedId": OTHER, "entries": [{"id": OTHER, "name": "Default"}]}).to_string(),
        )
        .unwrap();
        fs::write(
            paths.profile_file(OTHER),
            json!({"inferenceProvider": "bedrock", "inferenceBedrockRegion": "us-east-1"})
                .to_string(),
        )
        .unwrap();
        fs::write(
            paths.desktop_config(),
            json!({"deploymentMode": "1p", "preferences": {"sidebarMode": "chat"}}).to_string(),
        )
        .unwrap();
    }

    #[test]
    fn profile_ids_look_like_claudes() {
        let id = new_profile_id();
        assert_eq!(id.len(), 36);
        assert!(id.chars().all(|c| matches!(c, 'a'..='f' | '0'..='9' | '-')));
    }

    #[test]
    fn a_machine_that_never_used_third_party_mode_reads_as_empty() {
        let (_dir, paths) = sandbox();
        assert_eq!(snapshot(&paths), Snapshot::default());
    }

    #[test]
    fn apply_on_a_fresh_machine_writes_everything_claude_needs() {
        let (_dir, paths) = sandbox();
        let id = new_profile_id();

        apply_profile(&paths, &id, &oxen(Some("oxen-key"))).unwrap();

        assert_eq!(
            read(paths.profile_file(&id)),
            json!({
                "claudeAiImport": {"enabled": true},
                "inferenceProvider": "gateway",
                "inferenceGatewayBaseUrl": "https://hub.oxen.ai/api/ai",
                "inferenceGatewayApiKey": "oxen-key",
                "inferenceCredentialKind": "static"
            })
        );
        assert_eq!(
            read(paths.meta_file()),
            json!({"appliedId": id, "entries": [{"id": id, "name": "Oxen"}]})
        );
        assert_eq!(read(paths.desktop_config()), json!({"deploymentMode": "3p"}));
    }

    #[test]
    fn apply_leaves_other_profiles_and_settings_alone() {
        let (_dir, paths) = sandbox();
        with_existing_profile(&paths);
        let id = new_profile_id();

        apply_profile(&paths, &id, &oxen(Some("k"))).unwrap();

        let meta = read(paths.meta_file());
        assert_eq!(meta["appliedId"], json!(id));
        assert_eq!(meta["entries"][0], json!({"id": OTHER, "name": "Default"}));
        assert_eq!(meta["entries"].as_array().unwrap().len(), 2);
        assert_eq!(read(paths.profile_file(OTHER))["inferenceProvider"], "bedrock");
        assert_eq!(
            read(paths.desktop_config()),
            json!({"deploymentMode": "3p", "preferences": {"sidebarMode": "chat"}})
        );
    }

    #[test]
    fn applying_twice_does_not_duplicate_the_entry() {
        let (_dir, paths) = sandbox();
        let id = new_profile_id();
        apply_profile(&paths, &id, &oxen(Some("k"))).unwrap();
        apply_profile(&paths, &id, &oxen(Some("k"))).unwrap();
        assert_eq!(read(paths.meta_file())["entries"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn models_are_written_default_first_and_cleared_when_empty() {
        let (_dir, paths) = sandbox();
        let id = new_profile_id();
        let mut profile = oxen(Some("k"));
        profile.models = vec!["claude-opus".into(), "llama".into()];

        apply_profile(&paths, &id, &profile).unwrap();
        assert_eq!(
            read(paths.profile_file(&id))["inferenceModels"],
            json!(["claude-opus", "llama"])
        );

        profile.models.clear();
        apply_profile(&paths, &id, &profile).unwrap();
        assert!(read(paths.profile_file(&id)).get("inferenceModels").is_none());
    }

    #[test]
    fn a_model_with_a_label_is_written_the_way_claude_reads_it() {
        let (_dir, paths) = sandbox();
        let id = new_profile_id();
        let mut profile = oxen(Some("k"));
        profile.models = vec![
            "claude-opus-5-5".into(),
            ModelEntry {
                name: "claude-via-0b9f6c1e-7a52-4d3e-9f10-2c5a8e4b7d61".into(),
                label: Some("GPT 5 Mini".into()),
                behaves_as: Some("claude-sonnet-5".into()),
            },
        ];

        apply_profile(&paths, &id, &profile).unwrap();

        assert_eq!(
            read(paths.profile_file(&id))["inferenceModels"],
            json!([
                "claude-opus-5-5",
                {
                    "name": "claude-via-0b9f6c1e-7a52-4d3e-9f10-2c5a8e4b7d61",
                    "labelOverride": "GPT 5 Mini",
                    "behavesAs": "claude-sonnet-5"
                }
            ])
        );
        assert_eq!(
            snapshot(&paths).applied.unwrap().models,
            ["claude-opus-5-5", "claude-via-0b9f6c1e-7a52-4d3e-9f10-2c5a8e4b7d61"]
        );
    }

    #[test]
    fn a_key_claude_already_sealed_survives_an_update_without_a_key() {
        let (_dir, paths) = sandbox();
        let id = new_profile_id();
        apply_profile(&paths, &id, &oxen(Some("k"))).unwrap();
        // Claude seals the key at launch and may add keys of its own.
        let mut on_disk = read(paths.profile_file(&id));
        on_disk["inferenceGatewayApiKey"] = json!("__sealed__:v1:abc");
        on_disk["somethingClaudeAdded"] = json!(true);
        fs::write(paths.profile_file(&id), on_disk.to_string()).unwrap();

        let mut update = oxen(None);
        update.base_url = "http://localhost:3001/api/ai".into();
        apply_profile(&paths, &id, &update).unwrap();

        let after = read(paths.profile_file(&id));
        assert_eq!(after["inferenceGatewayApiKey"], "__sealed__:v1:abc");
        assert_eq!(after["inferenceGatewayBaseUrl"], "http://localhost:3001/api/ai");
        assert_eq!(after["somethingClaudeAdded"], true);
    }

    #[test]
    fn apply_without_any_key_is_refused() {
        let (_dir, paths) = sandbox();
        let id = new_profile_id();
        assert!(apply_profile(&paths, &id, &oxen(None)).is_err());
        assert!(!paths.profile_file(&id).exists());
    }

    #[test]
    fn snapshot_describes_the_applied_profile() {
        let (_dir, paths) = sandbox();
        let id = new_profile_id();
        let mut profile = oxen(Some("k"));
        profile.models = vec!["m1".into()];
        apply_profile(&paths, &id, &profile).unwrap();

        let snap = snapshot(&paths);
        assert_eq!(snap.applied_id.as_deref(), Some(id.as_str()));
        assert_eq!(snap.entry_ids, vec![id.clone()]);
        assert_eq!(snap.mode, Some(DeploymentMode::ThirdParty));
        assert_eq!(
            snap.applied,
            Some(ProfileView {
                provider: Some("gateway".into()),
                base_url: Some("https://hub.oxen.ai/api/ai".into()),
                has_key: true,
                models: vec!["m1".into()],
            })
        );
    }

    #[test]
    fn set_mode_changes_only_the_deployment_mode() {
        let (_dir, paths) = sandbox();
        with_existing_profile(&paths);

        set_mode(&paths, Some(DeploymentMode::ThirdParty)).unwrap();
        assert_eq!(
            read(paths.desktop_config()),
            json!({"deploymentMode": "3p", "preferences": {"sidebarMode": "chat"}})
        );

        set_mode(&paths, None).unwrap();
        assert_eq!(
            read(paths.desktop_config()),
            json!({"preferences": {"sidebarMode": "chat"}})
        );
    }

    #[test]
    fn a_desktop_config_we_cannot_parse_is_not_overwritten() {
        let (_dir, paths) = sandbox();
        fs::create_dir_all(&paths.data_dir).unwrap();
        fs::write(paths.desktop_config(), "{ not json").unwrap();

        assert!(set_mode(&paths, Some(DeploymentMode::ThirdParty)).is_err());
        assert_eq!(fs::read_to_string(paths.desktop_config()).unwrap(), "{ not json");
    }

    #[test]
    fn remove_restores_what_was_applied_before() {
        let (_dir, paths) = sandbox();
        with_existing_profile(&paths);
        let id = new_profile_id();
        apply_profile(&paths, &id, &oxen(Some("k"))).unwrap();

        remove_profile(&paths, &id, Some(OTHER), Some(DeploymentMode::Anthropic)).unwrap();

        assert!(!paths.profile_file(&id).exists());
        assert_eq!(
            read(paths.meta_file()),
            json!({"appliedId": OTHER, "entries": [{"id": OTHER, "name": "Default"}]})
        );
        assert_eq!(read(paths.desktop_config())["deploymentMode"], "1p");
    }

    #[test]
    fn remove_on_a_machine_that_only_had_hitch_leaves_no_third_party_config() {
        let (_dir, paths) = sandbox();
        let id = new_profile_id();
        apply_profile(&paths, &id, &oxen(Some("k"))).unwrap();

        remove_profile(&paths, &id, None, None).unwrap();

        assert!(!paths.library_dir().exists());
        assert_eq!(read(paths.desktop_config()), json!({}));
        assert_eq!(snapshot(&paths), Snapshot::default());
    }

    #[cfg(unix)]
    #[test]
    fn files_holding_the_key_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, paths) = sandbox();
        let id = new_profile_id();
        apply_profile(&paths, &id, &oxen(Some("k"))).unwrap();

        let mode = |p: std::path::PathBuf| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(paths.profile_file(&id)), 0o600);
        assert_eq!(mode(paths.library_dir()), 0o700);
    }
}

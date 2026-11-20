//! Commands the UI calls. Errors are sentences the UI shows as-is.

use std::sync::Arc;

use hitch_core::gateway::{self, Check};
use hitch_core::hitch::{Hitch, Status};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, State, WebviewWindow};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;

use crate::{tray, STATUS_EVENT};

type Shared<'a> = State<'a, Arc<Hitch>>;
type CommandResult<T> = Result<T, String>;

const POPOVER_WIDTH: f64 = 340.0;

#[tauri::command]
pub fn status(hitch: Shared) -> Status {
    hitch.status()
}

/// Checks the saved key against the gateway and brings Claude's model list up to date.
/// `None` when there is no key to check.
#[tauri::command]
pub async fn check_gateway(app: AppHandle, hitch: Shared<'_>) -> CommandResult<Option<Check>> {
    refresh(&app, hitch.inner()).await
}

/// Asks Oxen what it offers, remembers the answer, and rewrites Claude's model list if it
/// has fallen behind. Runs when Hitch starts, whenever the popover opens, and on Refresh.
pub async fn refresh(app: &AppHandle, hitch: &Arc<Hitch>) -> CommandResult<Option<Check>> {
    let reader = hitch.clone();
    let (key, base_url) = blocking(move || Ok((reader.api_key(), reader.status().base_url)))
        .await
        .map_err(sentence)?;
    let Some(key) = key else {
        return Ok(None);
    };
    let check = gateway::check(&base_url, &key).await;
    remember(hitch, &check).await;

    // A changed list means Claude needs a restart; the status sent here tells the UI.
    let syncing = hitch.clone();
    if let Ok(true) = blocking(move || syncing.sync_models()).await {
        let status = hitch.status();
        tray::reflect(app, &status);
        let _ = app.emit(STATUS_EVENT, &status);
    }
    Ok(Some(check))
}

/// Proves the key works before anything is written, so a typo never reaches Claude.
#[tauri::command]
pub async fn connect(
    app: AppHandle,
    hitch: Shared<'_>,
    api_key: Option<String>,
    base_url: Option<String>,
) -> CommandResult<Status> {
    let hitch = hitch.inner().clone();
    let api_key = api_key
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty());

    let url = match &base_url {
        Some(url) => gateway::normalize_base_url(url).map_err(sentence)?,
        None => hitch.status().base_url,
    };
    let stored = {
        let hitch = hitch.clone();
        blocking(move || Ok(hitch.api_key()))
            .await
            .map_err(sentence)?
    };
    let Some(key) = api_key.clone().or(stored) else {
        return Err("Paste your Oxen API key to connect.".into());
    };

    let check = gateway::check(&url, &key).await;
    // So the profile written below lists every model Oxen offers.
    remember(&hitch, &check).await;
    match check {
        Check::Ok { .. } => {}
        Check::InvalidKey => {
            return Err("Oxen didn't accept that key. Check it and try again.".into())
        }
        Check::Unreachable { message } | Check::Failed { message } => return Err(message),
    }

    let applied = {
        let hitch = hitch.clone();
        blocking(move || hitch.connect(api_key, Some(url))).await
    };
    changed(&app, applied)
}

#[tauri::command]
pub async fn set_routing(
    app: AppHandle,
    hitch: Shared<'_>,
    enabled: bool,
) -> CommandResult<Status> {
    let hitch = hitch.inner().clone();
    changed(&app, blocking(move || hitch.set_routing(enabled)).await)
}

#[tauri::command]
pub async fn set_models(
    app: AppHandle,
    hitch: Shared<'_>,
    models: Vec<String>,
) -> CommandResult<Status> {
    let hitch = hitch.inner().clone();
    // Refresh what Oxen offers first, so the names written to Claude are current. If Oxen
    // can't be reached, the last list Hitch saw is used.
    let reader = hitch.clone();
    let (key, base_url) = blocking(move || Ok((reader.api_key(), reader.status().base_url)))
        .await
        .map_err(sentence)?;
    if let Some(key) = key {
        remember(&hitch, &gateway::check(&base_url, &key).await).await;
    }
    changed(&app, blocking(move || hitch.set_models(models)).await)
}

#[tauri::command]
pub async fn remove(app: AppHandle, hitch: Shared<'_>) -> CommandResult<Status> {
    let hitch = hitch.inner().clone();
    changed(&app, blocking(move || hitch.remove()).await)
}

#[tauri::command]
pub async fn restart_claude(app: AppHandle, hitch: Shared<'_>) -> CommandResult<Status> {
    let hitch = hitch.inner().clone();
    changed(&app, blocking(move || hitch.restart_claude()).await)
}

/// Sends one tiny streaming message through the gateway, the call Claude depends on.
/// Returns the model that answered.
#[tauri::command]
pub async fn test_message(hitch: Shared<'_>) -> CommandResult<String> {
    let hitch = hitch.inner().clone();
    let (key, status) = blocking(move || Ok((hitch.api_key(), hitch.status())))
        .await
        .map_err(sentence)?;
    let key = key.ok_or("There is no API key to test with.")?;

    let model = match status.models.first() {
        Some(model) => model.clone(),
        None => match gateway::check(&status.base_url, &key).await {
            Check::Ok { models } => models
                .iter()
                .find(|m| m.id.contains("claude"))
                .or(models.first())
                .map(|m| m.id.clone())
                .ok_or("The gateway did not list any models.")?,
            Check::InvalidKey => return Err("Oxen didn't accept the key.".into()),
            Check::Unreachable { message } | Check::Failed { message } => return Err(message),
        },
    };

    gateway::probe_messages(&status.base_url, &key, &model)
        .await
        .map_err(sentence)?;
    Ok(model)
}

/// The last characters of the key already saved for a host, if there is one, so the UI
/// can offer it instead of asking for a paste.
#[tauri::command]
pub fn saved_key(hitch: Shared, host: String) -> Option<String> {
    hitch.saved_key_hint(&host)
}

/// Claude's app icon as a `data:` URL, read from the installed app. `None` without one.
#[tauri::command]
pub async fn claude_icon(hitch: Shared<'_>) -> CommandResult<Option<String>> {
    let hitch = hitch.inner().clone();
    blocking(move || Ok(hitch_core::claude::icon::icon_data_url(&hitch.paths)))
        .await
        .map_err(sentence)
}

#[tauri::command]
pub fn autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> CommandResult<bool> {
    let launcher = app.autolaunch();
    let result = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    result.map_err(|err| format!("Could not change the login setting: {err}"))?;
    Ok(launcher.is_enabled().unwrap_or(enabled))
}

/// Opens a web page in the browser. Only web links, so the UI can never launch anything else.
#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> CommandResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only web links can be opened.".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|err| err.to_string())
}

/// The popover is as tall as its content; the UI reports that height as it changes.
#[tauri::command]
pub fn resize(window: WebviewWindow, height: f64) -> CommandResult<()> {
    window
        .set_size(LogicalSize::new(POPOVER_WIDTH, height.clamp(120.0, 720.0)))
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn hide(window: WebviewWindow) {
    let _ = window.hide();
}

#[tauri::command]
pub fn quit(app: AppHandle) {
    app.exit(0);
}

/// Keeps Hitch's record of what Oxen offers current. Failing to save it is not worth
/// interrupting the user for; the previous record stays in use.
async fn remember(hitch: &Arc<Hitch>, check: &Check) {
    if let Check::Ok { models } = check {
        let (hitch, models) = (hitch.clone(), models.clone());
        let _ = blocking(move || hitch.remember_catalog(models)).await;
    }
}

/// After a change: update the menu-bar icon and tell the UI, without waiting for the poll.
fn changed(app: &AppHandle, result: hitch_core::Result<Status>) -> CommandResult<Status> {
    let status = match result {
        Ok(status) => status,
        Err(err) => {
            // A failed change can still have altered state; show what is true now.
            let hitch = app.state::<Arc<Hitch>>();
            let now = hitch.status();
            tray::reflect(app, &now);
            let _ = app.emit(STATUS_EVENT, &now);
            return Err(sentence(err));
        }
    };
    tray::reflect(app, &status);
    let _ = app.emit(STATUS_EVENT, &status);
    Ok(status)
}

/// File, keychain and process work blocks, so it runs off the async runtime's threads.
async fn blocking<T, F>(work: F) -> hitch_core::Result<T>
where
    T: Send + 'static,
    F: FnOnce() -> hitch_core::Result<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| hitch_core::Error::Process(err.to_string()))?
}

fn sentence(err: hitch_core::Error) -> String {
    err.to_string()
}

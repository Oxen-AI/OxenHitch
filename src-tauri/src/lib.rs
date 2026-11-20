//! The desktop shell: a menu-bar icon, a popover window, and the commands the UI calls.
//! All real work happens in `hitch-core`.

mod commands;
mod tray;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use hitch_core::claude::process::{fake::FakeClaude, SystemClaude};
use hitch_core::claude::{ClaudeApp, ClaudePaths};
use hitch_core::hitch::Hitch;
use hitch_core::secrets::{FileSecrets, Keychain, Remembered, SecretStore};
use hitch_core::state::StateStore;
use tauri::{Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;

/// Passed by the login item so starting with the Mac does not pop the window open.
const HIDDEN_FLAG: &str = "--hidden";
const STATUS_EVENT: &str = "status";
const POLL_EVERY: Duration = Duration::from_secs(4);

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![HIDDEN_FLAG]),
        ))
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let hitch = Arc::new(build_hitch(app.path().app_data_dir()?));
            app.manage(hitch.clone());
            tray::create(app.handle())?;

            let status = hitch.status();
            tray::reflect(app.handle(), &status);
            let launched_at_login = std::env::args().any(|arg| arg == HIDDEN_FLAG);
            if !launched_at_login && status.link == hitch_core::hitch::Link::NotConfigured {
                tray::show_when_ready(app.handle().clone());
            }

            watch(app.handle().clone(), hitch);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::check_gateway,
            commands::connect,
            commands::set_routing,
            commands::set_models,
            commands::remove,
            commands::restart_claude,
            commands::test_message,
            commands::autostart,
            commands::set_autostart,
            commands::open_url,
            commands::resize,
            commands::hide,
            commands::quit,
        ])
        .run(tauri::generate_context!())
        .expect("Hitch failed to start");
}

/// With `HITCH_CLAUDE_DIR` set, Hitch runs in a sandbox: it writes Claude config to that
/// directory, keeps its own state and key beside it, and never touches the real Claude
/// app or the real keychain.
fn build_hitch(app_data_dir: PathBuf) -> Hitch {
    let paths = ClaudePaths::detect();
    let sandbox = std::env::var_os("HITCH_CLAUDE_DIR").is_some();

    let (state_file, secrets, claude): (PathBuf, Box<dyn SecretStore>, Box<dyn ClaudeApp>) =
        if sandbox {
            (
                paths.data_dir.with_file_name("hitch-state.json"),
                Box::new(FileSecrets::new(paths.data_dir.with_file_name("hitch-key"))),
                Box::new(FakeClaude::running_since(1)),
            )
        } else {
            (
                app_data_dir.join("state.json"),
                key_store(&app_data_dir),
                Box::new(SystemClaude {
                    bundles: paths.app_bundles.clone(),
                }),
            )
        };

    Hitch {
        paths,
        state: StateStore::new(state_file),
        secrets,
        claude,
    }
}

const KEYCHAIN_SERVICE: &str = "ai.oxen.hitch";

/// Where the Oxen API key is kept.
///
/// Release builds use the OS keychain, read once per launch. A signed app reads its own
/// keychain item without a prompt.
///
/// Development builds use an owner-only file instead. They are unsigned and every rebuild
/// looks like a different app to macOS, so the keychain would ask for permission after
/// each code change. Development builds never touch the keychain, not even to copy over a
/// key an earlier build saved there: that read would raise the same prompt.
fn key_store(app_data_dir: &std::path::Path) -> Box<dyn SecretStore> {
    if cfg!(debug_assertions) {
        Box::new(Remembered::new(FileSecrets::new(app_data_dir.join("dev-key"))))
    } else {
        Box::new(Remembered::new(Keychain::new(KEYCHAIN_SERVICE)))
    }
}

/// Keeps the menu-bar icon and an open popover current when things change outside Hitch:
/// Claude quits or starts, or its config is edited.
fn watch(app: tauri::AppHandle, hitch: Arc<Hitch>) {
    std::thread::spawn(move || {
        let mut last = None;
        loop {
            let status = hitch.status();
            if last.as_ref() != Some(&status) {
                tray::reflect(&app, &status);
                let _ = app.emit(STATUS_EVENT, &status);
                last = Some(status);
            }
            std::thread::sleep(POLL_EVERY);
        }
    });
}

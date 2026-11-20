//! Finding, quitting and reopening the Claude Desktop app.

use std::path::PathBuf;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

use crate::{Error, Result};

pub trait ClaudeApp: Send + Sync {
    fn installed(&self) -> bool;
    /// When the running app started (unix seconds), or `None` if it is not running.
    fn started_at(&self) -> Option<u64>;
    /// Quits the app if it is running, then opens it.
    fn restart(&self) -> Result<()>;
}

/// The real app on this machine.
pub struct SystemClaude {
    pub bundles: Vec<PathBuf>,
}

const BUNDLE_ID: &str = "com.anthropic.claudefordesktop";
const QUIT_TIMEOUT: Duration = Duration::from_secs(15);

impl SystemClaude {
    /// The main process is the one whose executable sits directly in an installed bundle.
    /// Helper processes live deeper, under `Frameworks/`.
    fn main_executables(&self) -> Vec<PathBuf> {
        self.bundles
            .iter()
            .map(|bundle| {
                if cfg!(target_os = "macos") {
                    bundle.join("Contents/MacOS/Claude")
                } else if cfg!(target_os = "windows") {
                    bundle.join("claude.exe")
                } else {
                    bundle.clone()
                }
            })
            .collect()
    }
}

impl ClaudeApp for SystemClaude {
    fn installed(&self) -> bool {
        self.bundles.iter().any(|bundle| bundle.exists())
    }

    fn started_at(&self) -> Option<u64> {
        let executables = self.main_executables();
        let mut system = System::new();
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::OnlyIfNotSet),
        );
        system
            .processes()
            .values()
            .filter(|process| process.exe().is_some_and(|exe| executables.iter().any(|e| e == exe)))
            .map(|process| process.start_time())
            .min()
    }

    fn restart(&self) -> Result<()> {
        if self.started_at().is_some() {
            quit()?;
            let deadline = Instant::now() + QUIT_TIMEOUT;
            while self.started_at().is_some() {
                if Instant::now() > deadline {
                    return Err(Error::Process(
                        "Claude did not quit. It may be waiting on a question — check its windows, then try again.".into(),
                    ));
                }
                sleep(Duration::from_millis(200));
            }
        }
        open()
    }
}

#[cfg(target_os = "macos")]
fn quit() -> Result<()> {
    // A normal quit, the same as choosing Quit from the menu, so Claude can save its state.
    run(Command::new("osascript")
        .arg("-e")
        .arg(format!("tell application id \"{BUNDLE_ID}\" to quit")))
}

#[cfg(target_os = "macos")]
fn open() -> Result<()> {
    run(Command::new("open").arg("-b").arg(BUNDLE_ID))
}

#[cfg(not(target_os = "macos"))]
fn quit() -> Result<()> {
    let _ = BUNDLE_ID;
    Err(Error::Process("Please quit Claude yourself, then open it again.".into()))
}

#[cfg(not(target_os = "macos"))]
fn open() -> Result<()> {
    Err(Error::Process("Please open Claude yourself.".into()))
}

#[allow(dead_code)]
fn run(command: &mut Command) -> Result<()> {
    let output = command
        .output()
        .map_err(|err| Error::Process(format!("Could not control Claude: {err}")))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(Error::Process(format!(
            "Could not control Claude: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// A stand-in for tests.
pub mod fake {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    /// `started_at == 0` means not running.
    pub struct FakeClaude {
        pub installed: AtomicBool,
        pub started_at: AtomicU64,
        /// What `started_at` becomes when restarted.
        pub restart_time: AtomicU64,
    }

    impl FakeClaude {
        pub fn running_since(started_at: u64) -> Self {
            Self {
                installed: AtomicBool::new(true),
                started_at: AtomicU64::new(started_at),
                restart_time: AtomicU64::new(u64::MAX),
            }
        }

        pub fn not_running() -> Self {
            Self::running_since(0)
        }
    }

    impl ClaudeApp for FakeClaude {
        fn installed(&self) -> bool {
            self.installed.load(Ordering::SeqCst)
        }

        fn started_at(&self) -> Option<u64> {
            Some(self.started_at.load(Ordering::SeqCst)).filter(|&t| t > 0)
        }

        fn restart(&self) -> Result<()> {
            self.started_at
                .store(self.restart_time.load(Ordering::SeqCst), Ordering::SeqCst);
            Ok(())
        }
    }
}

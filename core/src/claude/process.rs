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
/// How long to wait for Claude to finish quitting, and again for it to finish opening.
const PATIENCE: Duration = Duration::from_secs(20);

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
        restart_with(
            || self.started_at().is_some(),
            quit,
            open,
            PATIENCE,
            Duration::from_millis(200),
        )
    }
}

/// Quit Claude if it is running, wait until it is really gone, open it, and wait until
/// it is really back. Waiting matters at both ends: opening too early just brings the
/// old process forward with its old settings, and returning too early leaves the user
/// looking at "Claude isn't running".
fn restart_with(
    is_running: impl Fn() -> bool,
    quit: impl FnOnce() -> Result<()>,
    open: impl FnOnce() -> Result<()>,
    patience: Duration,
    poll: Duration,
) -> Result<()> {
    let wait_until = |wanted: bool| {
        let deadline = Instant::now() + patience;
        while is_running() != wanted {
            if Instant::now() > deadline {
                return false;
            }
            sleep(poll);
        }
        true
    };

    if is_running() {
        quit()?;
        if !wait_until(false) {
            return Err(Error::Process(
                "Claude did not quit. It may be asking you something — check its windows, then try again.".into(),
            ));
        }
    }
    open()?;
    if !wait_until(true) {
        return Err(Error::Process(
            "Claude did not open. Try opening it yourself.".into(),
        ));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    const PATIENCE: Duration = Duration::from_millis(300);
    const POLL: Duration = Duration::from_millis(1);

    /// A Claude that takes a few checks to react, like the real one.
    struct Scripted {
        running: Cell<bool>,
        /// Checks left before a requested quit or open takes effect.
        delay: Cell<u32>,
        pending: Cell<Option<bool>>,
        log: RefCell<Vec<&'static str>>,
        obeys_quit: bool,
        obeys_open: bool,
    }

    impl Scripted {
        fn new(running: bool) -> Self {
            Self {
                running: Cell::new(running),
                delay: Cell::new(0),
                pending: Cell::new(None),
                log: RefCell::new(Vec::new()),
                obeys_quit: true,
                obeys_open: true,
            }
        }

        fn is_running(&self) -> bool {
            if let Some(next) = self.pending.get() {
                if self.delay.get() == 0 {
                    self.running.set(next);
                    self.pending.set(None);
                } else {
                    self.delay.set(self.delay.get() - 1);
                }
            }
            self.running.get()
        }

        fn request(&self, name: &'static str, obeyed: bool, becomes: bool) -> Result<()> {
            self.log.borrow_mut().push(name);
            if obeyed {
                self.pending.set(Some(becomes));
                self.delay.set(3);
            }
            Ok(())
        }

        fn restart(&self) -> Result<()> {
            restart_with(
                || self.is_running(),
                || self.request("quit", self.obeys_quit, false),
                || {
                    assert!(!self.running.get(), "opened while the old Claude was still running");
                    self.request("open", self.obeys_open, true)
                },
                PATIENCE,
                POLL,
            )
        }
    }

    #[test]
    fn a_running_claude_is_quit_then_opened_and_is_back_when_we_return() {
        let claude = Scripted::new(true);
        claude.restart().unwrap();
        assert_eq!(*claude.log.borrow(), ["quit", "open"]);
        assert!(claude.running.get(), "restart returned before Claude was back");
    }

    #[test]
    fn a_claude_that_is_not_running_is_just_opened() {
        let claude = Scripted::new(false);
        claude.restart().unwrap();
        assert_eq!(*claude.log.borrow(), ["open"]);
        assert!(claude.running.get());
    }

    #[test]
    fn a_claude_that_will_not_quit_is_left_alone() {
        let mut claude = Scripted::new(true);
        claude.obeys_quit = false;
        let err = claude.restart().unwrap_err();
        assert_eq!(*claude.log.borrow(), ["quit"], "must not open a second copy");
        assert!(err.to_string().contains("did not quit"));
    }

    #[test]
    fn a_claude_that_never_opens_is_reported() {
        let mut claude = Scripted::new(false);
        claude.obeys_open = false;
        let err = claude.restart().unwrap_err();
        assert!(err.to_string().contains("did not open"));
    }
}

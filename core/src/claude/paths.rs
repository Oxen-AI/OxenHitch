use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Where Claude Desktop keeps its third-party ("3p") configuration.
#[derive(Debug, Clone)]
pub struct ClaudePaths {
    /// The `Claude-3p` data directory.
    pub data_dir: PathBuf,
    /// Admin-managed config sources. When one sets inference keys, local config is ignored.
    pub managed_sources: Vec<PathBuf>,
    /// Places the app itself may be installed.
    pub app_bundles: Vec<PathBuf>,
}

const DATA_DIR_NAME: &str = "Claude-3p";
const BUNDLE_ID: &str = "com.anthropic.claudefordesktop";

/// Keys that, when present in a managed source, mean an admin controls inference.
const MANAGED_MARKERS: [&str; 2] = ["inferenceProvider", "bootstrapUrl"];

impl ClaudePaths {
    /// Paths for a specific data directory, with nothing else on the system. Used by tests.
    pub fn at(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            managed_sources: Vec::new(),
            app_bundles: Vec::new(),
        }
    }

    /// Paths for this machine. `HITCH_CLAUDE_DIR` overrides the data directory so the app
    /// can be tried out without touching real Claude config.
    pub fn detect() -> Self {
        let mut paths = Self::platform_default();
        if let Some(dir) = env::var_os("HITCH_CLAUDE_DIR") {
            paths.data_dir = PathBuf::from(dir);
            paths.managed_sources.clear();
        }
        paths
    }

    #[cfg(target_os = "macos")]
    fn platform_default() -> Self {
        let home = home_dir();
        let managed = Path::new("/Library/Managed Preferences");
        let plist = format!("{BUNDLE_ID}.plist");
        let mut managed_sources = vec![managed.join(&plist)];
        if let Some(user) = env::var_os("USER") {
            managed_sources.push(managed.join(user).join(&plist));
        }
        Self {
            data_dir: home.join("Library/Application Support").join(DATA_DIR_NAME),
            managed_sources,
            app_bundles: vec![
                PathBuf::from("/Applications/Claude.app"),
                home.join("Applications/Claude.app"),
            ],
        }
    }

    #[cfg(target_os = "windows")]
    fn platform_default() -> Self {
        let _ = BUNDLE_ID;
        let local = env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join("AppData/Local"));
        Self {
            data_dir: local.join(DATA_DIR_NAME),
            // Windows policy lives in the registry (HKLM/HKCU\SOFTWARE\Policies\Claude); not read yet.
            managed_sources: Vec::new(),
            app_bundles: vec![local.join("AnthropicClaude")],
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    fn platform_default() -> Self {
        let _ = BUNDLE_ID;
        let config = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home_dir().join(".config"));
        Self {
            data_dir: config.join(DATA_DIR_NAME),
            managed_sources: vec![PathBuf::from("/etc/claude-desktop/managed-settings.json")],
            app_bundles: vec![PathBuf::from("/usr/bin/claude-desktop")],
        }
    }

    pub fn library_dir(&self) -> PathBuf {
        self.data_dir.join("configLibrary")
    }

    pub fn meta_file(&self) -> PathBuf {
        self.library_dir().join("_meta.json")
    }

    pub fn profile_file(&self, id: &str) -> PathBuf {
        self.library_dir().join(format!("{id}.json"))
    }

    /// Holds `deploymentMode`, the user's choice between Anthropic and third-party mode.
    pub fn desktop_config(&self) -> PathBuf {
        self.data_dir.join("claude_desktop_config.json")
    }

    pub fn installed(&self) -> bool {
        self.app_bundles.iter().any(|p| p.exists())
    }

    /// True when an admin-managed source controls inference, which makes Claude ignore
    /// anything Hitch writes.
    pub fn managed(&self) -> bool {
        self.managed_sources.iter().any(|p| mentions_inference(p))
    }
}

/// Managed sources are plists (often binary) or JSON. In both, key names appear as plain
/// bytes, so a byte search is enough to tell whether inference is managed.
fn mentions_inference(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    MANAGED_MARKERS
        .iter()
        .any(|m| bytes.windows(m.len()).any(|w| w == m.as_bytes()))
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .or_else(|| env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_only_when_a_source_sets_inference() {
        let dir = tempfile::tempdir().unwrap();
        let plist = dir.path().join("managed.plist");
        let mut paths = ClaudePaths::at(dir.path());
        paths.managed_sources = vec![plist.clone()];

        assert!(!paths.managed(), "a missing source is not managed");

        fs::write(&plist, b"bplist00\x01claudeAiImport\x02").unwrap();
        assert!(
            !paths.managed(),
            "app-behavior keys alone do not manage inference"
        );

        fs::write(&plist, b"bplist00\x01inferenceProvider\x02gateway").unwrap();
        assert!(paths.managed());
    }

    #[test]
    fn profile_lives_in_the_config_library() {
        let paths = ClaudePaths::at("/x/Claude-3p");
        assert_eq!(
            paths.profile_file("abc"),
            PathBuf::from("/x/Claude-3p/configLibrary/abc.json")
        );
        assert_eq!(
            paths.meta_file(),
            PathBuf::from("/x/Claude-3p/configLibrary/_meta.json")
        );
    }
}

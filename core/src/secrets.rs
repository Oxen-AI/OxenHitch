//! Where Hitch keeps the user's Oxen API key.
//!
//! Claude seals its own copy of the key so it cannot be read back. Hitch needs the key to
//! check the connection, list models, and rewrite Claude's profile. It keeps it where the
//! Oxen CLI already does: `~/.config/oxen/auth_config.toml`, one token per host. Someone
//! who has used the CLI never has to paste a key, and a key pasted into Hitch works in the
//! CLI too. No keychain means no permission prompts.

use std::collections::HashMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::PathBuf;
use std::sync::Mutex;

use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table};

use crate::{Error, Result};

/// Keys are stored per host (`hub.oxen.ai`, `localhost:3001`), the way the Oxen CLI does.
pub trait SecretStore: Send + Sync {
    fn get(&self, host: &str) -> Result<Option<String>>;
    fn set(&self, host: &str, key: &str) -> Result<()>;
}

/// The Oxen CLI's auth file.
pub struct OxenConfig {
    path: PathBuf,
}

impl OxenConfig {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The file the Oxen CLI reads on this machine.
    pub fn for_user() -> Self {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self::new(home.join(".config/oxen/auth_config.toml"))
    }
}

const HOSTS: &str = "host_configs";
const HOST: &str = "host";
const TOKEN: &str = "auth_token";
/// The host the CLI assumes when none is named; written when Hitch creates the file.
const CLI_DEFAULT_HOST: &str = "hub.oxen.ai";

impl SecretStore for OxenConfig {
    fn get(&self, host: &str) -> Result<Option<String>> {
        let Ok(text) = fs::read_to_string(&self.path) else {
            return Ok(None);
        };
        let Ok(document) = text.parse::<DocumentMut>() else {
            return Ok(None);
        };
        let token = document
            .get(HOSTS)
            .and_then(Item::as_array_of_tables)
            .and_then(|hosts| hosts.iter().find(|entry| names(entry, host)))
            .and_then(|entry| entry.get(TOKEN))
            .and_then(Item::as_str)
            .filter(|token| !token.is_empty())
            .map(String::from);
        Ok(token)
    }

    fn set(&self, host: &str, key: &str) -> Result<()> {
        let mut document = match fs::read_to_string(&self.path) {
            // Editing in place keeps the CLI's other hosts, its default host, and any
            // comments exactly as they were.
            Ok(text) => text.parse::<DocumentMut>().map_err(|_| {
                Error::invalid(format!(
                    "Your Oxen settings file could not be read, so the key was not saved: {}",
                    self.path.display()
                ))
            })?,
            Err(err) if err.kind() == ErrorKind::NotFound => {
                let mut fresh = DocumentMut::new();
                fresh["default_host"] = value(CLI_DEFAULT_HOST);
                fresh
            }
            Err(err) => return Err(err.into()),
        };

        let hosts = document
            .entry(HOSTS)
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .ok_or_else(|| Error::invalid("Your Oxen settings file has an unexpected layout."))?;
        let existing = hosts.iter().position(|entry| names(entry, host));
        match existing.and_then(|index| hosts.get_mut(index)) {
            Some(entry) => entry[TOKEN] = value(key),
            None => {
                let mut entry = Table::new();
                entry[HOST] = value(host);
                entry[TOKEN] = value(key);
                hosts.push(entry);
            }
        }

        self.write(&document.to_string())
    }
}

impl OxenConfig {
    /// Atomic, and owner-only: this file holds API keys.
    fn write(&self, text: &str) -> Result<()> {
        let dir = self.path.parent().expect("the auth file lives in a directory");
        fs::create_dir_all(dir)?;
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
            fs::rename(&temp, &self.path)
        })();
        if written.is_err() {
            let _ = fs::remove_file(&temp);
        }
        Ok(written?)
    }
}

fn names(entry: &Table, host: &str) -> bool {
    entry.get(HOST).and_then(Item::as_str) == Some(host)
}

/// Keeps keys in memory only. For tests.
#[derive(Default)]
pub struct MemorySecrets(Mutex<HashMap<String, String>>);

impl SecretStore for MemorySecrets {
    fn get(&self, host: &str) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().get(host).cloned())
    }

    fn set(&self, host: &str, key: &str) -> Result<()> {
        self.0.lock().unwrap().insert(host.into(), key.into());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLI_FILE: &str = r#"default_host = "hub.oxen.ai"

[[host_configs]]
host = "localhost:3001"
auth_token = "local-token"

[[host_configs]]
host = "hub.oxen.ai"
auth_token = "hub-token"
"#;

    fn store(contents: Option<&str>) -> (tempfile::TempDir, OxenConfig, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("oxen/auth_config.toml");
        if let Some(contents) = contents {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, contents).unwrap();
        }
        (dir, OxenConfig::new(&path), path)
    }

    #[test]
    fn the_key_the_cli_saved_for_a_host_is_found() {
        let (_dir, config, _) = store(Some(CLI_FILE));
        assert_eq!(config.get("hub.oxen.ai").unwrap().as_deref(), Some("hub-token"));
        assert_eq!(config.get("localhost:3001").unwrap().as_deref(), Some("local-token"));
    }

    #[test]
    fn a_host_the_cli_has_never_used_has_no_key() {
        let (_dir, config, _) = store(Some(CLI_FILE));
        assert_eq!(config.get("staging.hub.oxen.ai").unwrap(), None);
    }

    #[test]
    fn someone_who_never_used_the_cli_has_no_key() {
        let (_dir, config, _) = store(None);
        assert_eq!(config.get("hub.oxen.ai").unwrap(), None);
    }

    #[test]
    fn a_file_that_cannot_be_read_as_toml_has_no_key() {
        let (_dir, config, _) = store(Some("not = [toml"));
        assert_eq!(config.get("hub.oxen.ai").unwrap(), None);
    }

    #[test]
    fn saving_a_key_replaces_only_that_hosts_token() {
        let (_dir, config, path) = store(Some(CLI_FILE));

        config.set("hub.oxen.ai", "new-hub-token").unwrap();

        assert_eq!(config.get("hub.oxen.ai").unwrap().as_deref(), Some("new-hub-token"));
        assert_eq!(config.get("localhost:3001").unwrap().as_deref(), Some("local-token"));
        let text = fs::read_to_string(path).unwrap();
        assert!(text.contains(r#"default_host = "hub.oxen.ai""#));
        assert_eq!(text.matches("[[host_configs]]").count(), 2, "no duplicate entry");
    }

    #[test]
    fn saving_a_key_for_a_new_host_adds_it_beside_the_others() {
        let (_dir, config, path) = store(Some(CLI_FILE));

        config.set("dev.hub.oxen.ai", "dev-token").unwrap();

        assert_eq!(config.get("dev.hub.oxen.ai").unwrap().as_deref(), Some("dev-token"));
        assert_eq!(config.get("hub.oxen.ai").unwrap().as_deref(), Some("hub-token"));
        assert_eq!(fs::read_to_string(path).unwrap().matches("[[host_configs]]").count(), 3);
    }

    #[test]
    fn saving_a_key_creates_the_file_the_cli_expects() {
        let (_dir, config, path) = store(None);

        config.set("hub.oxen.ai", "first-token").unwrap();

        assert_eq!(config.get("hub.oxen.ai").unwrap().as_deref(), Some("first-token"));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(r#"default_host = "hub.oxen.ai""#));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    #[test]
    fn a_file_that_cannot_be_read_as_toml_is_not_overwritten() {
        let (_dir, config, path) = store(Some("not = [toml"));
        assert!(config.set("hub.oxen.ai", "k").is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "not = [toml");
    }

    #[test]
    fn saving_keeps_the_file_private_even_if_it_was_not() {
        let (_dir, config, path) = store(Some(CLI_FILE));
        config.set("hub.oxen.ai", "k").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}

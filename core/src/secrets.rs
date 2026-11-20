//! Where Hitch keeps the user's Oxen API key.
//!
//! Claude seals its own copy of the key so it cannot be read back. Hitch needs the key to
//! check the connection and to rewrite Claude's profile, so it keeps one in the OS keychain.

use std::sync::Mutex;

use crate::{Error, Result};

pub trait SecretStore: Send + Sync {
    fn get(&self) -> Result<Option<String>>;
    fn set(&self, key: &str) -> Result<()>;
    fn delete(&self) -> Result<()>;
}

/// The OS keychain: Keychain on macOS, Credential Manager on Windows.
pub struct Keychain {
    service: String,
}

const ACCOUNT: &str = "oxen-api-key";

impl Keychain {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    fn entry(&self) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, ACCOUNT).map_err(keychain_error)
    }
}

impl SecretStore for Keychain {
    fn get(&self) -> Result<Option<String>> {
        match self.entry()?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(keychain_error(err)),
        }
    }

    fn set(&self, key: &str) -> Result<()> {
        self.entry()?.set_password(key).map_err(keychain_error)
    }

    fn delete(&self) -> Result<()> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(keychain_error(err)),
        }
    }
}

fn keychain_error(err: keyring::Error) -> Error {
    Error::Keychain(err.to_string())
}

/// Keeps the key in memory only. For tests.
#[derive(Default)]
pub struct MemorySecrets(Mutex<Option<String>>);

impl SecretStore for MemorySecrets {
    fn get(&self) -> Result<Option<String>> {
        Ok(self.0.lock().unwrap().clone())
    }

    fn set(&self, key: &str) -> Result<()> {
        *self.0.lock().unwrap() = Some(key.into());
        Ok(())
    }

    fn delete(&self) -> Result<()> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}

/// Remembers the key after the first read, so the OS keychain is asked at most once
/// per launch. Each keychain read can put a permission prompt in front of the user.
pub struct Remembered<S> {
    inner: S,
    known: Mutex<Option<Option<String>>>,
}

impl<S: SecretStore> Remembered<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            known: Mutex::new(None),
        }
    }
}

impl<S: SecretStore> SecretStore for Remembered<S> {
    fn get(&self) -> Result<Option<String>> {
        let mut known = self.known.lock().unwrap();
        if let Some(key) = known.as_ref() {
            return Ok(key.clone());
        }
        // A failed read is not remembered: the user may have denied the prompt by
        // mistake, and the next attempt should be allowed to ask again.
        let key = self.inner.get()?;
        *known = Some(key.clone());
        Ok(key)
    }

    fn set(&self, key: &str) -> Result<()> {
        self.inner.set(key)?;
        *self.known.lock().unwrap() = Some(Some(key.into()));
        Ok(())
    }

    fn delete(&self) -> Result<()> {
        self.inner.delete()?;
        *self.known.lock().unwrap() = Some(None);
        Ok(())
    }
}

/// Keeps the key in an owner-only file. For sandbox mode, where a throwaway key should
/// not touch the real keychain or raise its prompts.
pub struct FileSecrets {
    path: std::path::PathBuf,
}

impl FileSecrets {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl SecretStore for FileSecrets {
    fn get(&self) -> Result<Option<String>> {
        match std::fs::read_to_string(&self.path) {
            Ok(key) => Ok(Some(key).filter(|key| !key.is_empty())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    fn set(&self, key: &str) -> Result<()> {
        use std::io::Write;
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&self.path)?.write_all(key.as_bytes())?;
        Ok(())
    }

    fn delete(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Err(err) if err.kind() != std::io::ErrorKind::NotFound => Err(err.into()),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A keychain that counts how often it is read, and can refuse.
    #[derive(Default)]
    struct Counting {
        store: MemorySecrets,
        reads: Arc<AtomicUsize>,
        locked: bool,
    }

    impl SecretStore for Counting {
        fn get(&self) -> Result<Option<String>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.locked {
                return Err(Error::Keychain("denied".into()));
            }
            self.store.get()
        }
        fn set(&self, key: &str) -> Result<()> {
            self.store.set(key)
        }
        fn delete(&self) -> Result<()> {
            self.store.delete()
        }
    }

    fn counted(initial: Option<&str>) -> (Remembered<Counting>, Arc<AtomicUsize>) {
        let inner = Counting::default();
        if let Some(key) = initial {
            inner.store.set(key).unwrap();
        }
        let reads = inner.reads.clone();
        (Remembered::new(inner), reads)
    }

    #[test]
    fn the_keychain_is_read_once_however_often_the_key_is_needed() {
        let (secrets, reads) = counted(Some("k"));
        for _ in 0..5 {
            assert_eq!(secrets.get().unwrap().as_deref(), Some("k"));
        }
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn an_empty_keychain_is_also_only_read_once() {
        let (secrets, reads) = counted(None);
        assert_eq!(secrets.get().unwrap(), None);
        assert_eq!(secrets.get().unwrap(), None);
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn saving_a_key_means_it_never_has_to_be_read_back() {
        let (secrets, reads) = counted(None);
        secrets.set("new").unwrap();
        assert_eq!(secrets.get().unwrap().as_deref(), Some("new"));
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        assert_eq!(secrets.inner.store.get().unwrap().as_deref(), Some("new"));
    }

    #[test]
    fn deleting_forgets_the_key_everywhere() {
        let (secrets, reads) = counted(Some("k"));
        secrets.get().unwrap();
        secrets.delete().unwrap();
        assert_eq!(secrets.get().unwrap(), None);
        assert_eq!(secrets.inner.store.get().unwrap(), None);
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_refused_read_is_tried_again_next_time() {
        let inner = Counting {
            locked: true,
            ..Default::default()
        };
        let reads = inner.reads.clone();
        let secrets = Remembered::new(inner);
        assert!(secrets.get().is_err());
        assert!(secrets.get().is_err());
        assert_eq!(reads.load(Ordering::SeqCst), 2, "a refusal must not be remembered");
    }

    #[test]
    fn a_key_file_round_trips_and_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/key");
        let secrets = FileSecrets::new(&path);
        assert_eq!(secrets.get().unwrap(), None);

        secrets.set("oxen-key").unwrap();
        assert_eq!(secrets.get().unwrap().as_deref(), Some("oxen-key"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        secrets.delete().unwrap();
        assert_eq!(secrets.get().unwrap(), None);
        secrets.delete().unwrap();
    }
}

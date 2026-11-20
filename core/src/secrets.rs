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

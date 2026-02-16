// Keychain-backed secret storage for sensitive credentials.
// Uses macOS Keychain via the `keyring` crate to keep tokens and secrets
// encrypted and protected by the OS, rather than storing them in plain SQLite.

use anyhow::{Context, Result};
use log::warn;

const SERVICE: &str = "com.pyr.reader";

/// Secret keys that should live in Keychain, not SQLite.
const SECRET_KEYS: &[&str] = &[
    "anthropic_api_key",
    "openai_api_key",
    "tavily_api_key",
];

/// Manages secrets stored in macOS Keychain.
pub struct SecretStore;

impl SecretStore {
    /// Store a secret in the Keychain.
    pub fn set(key: &str, value: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE, key)
            .context(format!("Failed to create keyring entry for '{}'", key))?;
        entry
            .set_password(value)
            .context(format!("Failed to save secret '{}'", key))?;
        Ok(())
    }

    /// Retrieve a secret from the Keychain. Returns None if not found.
    pub fn get(key: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(SERVICE, key)
            .context(format!("Failed to create keyring entry for '{}'", key))?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(anyhow::anyhow!(
                "Failed to retrieve secret '{}': {}",
                key,
                e
            )),
        }
    }

    /// Delete a secret from the Keychain. No-op if not found.
    pub fn delete(key: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE, key)
            .context(format!("Failed to create keyring entry for '{}'", key))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(anyhow::anyhow!(
                "Failed to delete secret '{}': {}",
                key,
                e
            )),
        }
    }

    /// Returns true if the given key is a secret that belongs in Keychain.
    pub fn is_secret_key(key: &str) -> bool {
        SECRET_KEYS.contains(&key)
    }

    /// Migrate secrets from SQLite settings to Keychain.
    /// For each secret key: if it exists in SQLite and NOT yet in Keychain,
    /// copy it to Keychain, then delete from SQLite.
    /// Idempotent — safe to call on every startup.
    pub fn migrate_from_sqlite(storage: &super::StorageManager) {
        for key in SECRET_KEYS {
            // Check if the secret exists in SQLite.
            let sqlite_value = match storage.get_setting(key) {
                Ok(Some(val)) => val,
                _ => continue,
            };

            // Check if already in Keychain.
            match Self::get(key) {
                Ok(Some(_)) => {
                    // Already migrated. Delete the SQLite copy.
                    if let Err(e) = storage.delete_setting(key) {
                        warn!("Failed to delete migrated setting '{}' from SQLite: {}", key, e);
                    }
                    continue;
                }
                Ok(None) => {} // Not in Keychain yet; proceed.
                Err(e) => {
                    warn!("Failed to check Keychain for '{}': {}. Skipping migration.", key, e);
                    continue;
                }
            }

            // Write to Keychain.
            if let Err(e) = Self::set(key, &sqlite_value) {
                warn!("Failed to migrate '{}' to Keychain: {}. Secret remains in SQLite.", key, e);
                continue;
            }

            // Delete from SQLite only after successful Keychain write.
            if let Err(e) = storage.delete_setting(key) {
                warn!("Failed to delete migrated setting '{}' from SQLite: {}", key, e);
            }
        }
    }
}

#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

#[cfg(not(test))]
use keyring::Entry;
#[cfg(not(test))]
use keyring::Error;

#[cfg(not(test))]
const SERVICE_NAME: &str = "Ark Git GUI";

pub struct CredentialStore;

#[cfg(test)]
fn test_store() -> &'static Mutex<HashMap<String, String>> {
    static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

impl CredentialStore {
    #[cfg(test)]
    pub fn clear_all() {
        test_store()
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .clear();
    }

    #[cfg(test)]
    fn get_passphrase_impl(key_path: &str) -> Result<Option<String>, String> {
        Ok(test_store()
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .get(key_path)
            .cloned())
    }

    #[cfg(not(test))]
    fn entry(key_path: &str) -> Result<Entry, String> {
        Entry::new(SERVICE_NAME, key_path).map_err(|err| err.to_string())
    }

    #[cfg(not(test))]
    fn get_passphrase_impl(key_path: &str) -> Result<Option<String>, String> {
        let entry = Self::entry(key_path)?;
        match entry.get_password() {
            Ok(passphrase) => Ok(Some(passphrase)),
            Err(Error::NoEntry) => Ok(None),
            Err(err) => Err(err.to_string()),
        }
    }

    #[cfg(test)]
    fn set_passphrase_impl(key_path: &str, passphrase: &str) -> Result<(), String> {
        test_store()
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .insert(key_path.to_string(), passphrase.to_string());
        Ok(())
    }

    #[cfg(not(test))]
    fn set_passphrase_impl(key_path: &str, passphrase: &str) -> Result<(), String> {
        Self::entry(key_path)?
            .set_password(passphrase)
            .map_err(|err| err.to_string())
    }

    #[cfg(test)]
    fn delete_passphrase_impl(key_path: &str) -> Result<(), String> {
        test_store()
            .lock()
            .unwrap_or_else(|err| err.into_inner())
            .remove(key_path);
        Ok(())
    }

    #[cfg(not(test))]
    fn delete_passphrase_impl(key_path: &str) -> Result<(), String> {
        let entry = Self::entry(key_path)?;
        match entry.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    }

    pub fn get_passphrase(key_path: &str) -> Result<Option<String>, String> {
        if key_path.trim().is_empty() {
            return Ok(None);
        }

        Self::get_passphrase_impl(key_path)
    }

    pub fn set_passphrase(key_path: &str, passphrase: &str) -> Result<(), String> {
        if key_path.trim().is_empty() {
            return Err("SSH key path is required to store passphrase".to_string());
        }

        Self::set_passphrase_impl(key_path, passphrase)
    }

    pub fn delete_passphrase(key_path: &str) -> Result<(), String> {
        if key_path.trim().is_empty() {
            return Ok(());
        }

        Self::delete_passphrase_impl(key_path)
    }
}

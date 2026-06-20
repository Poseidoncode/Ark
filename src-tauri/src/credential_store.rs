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

#[cfg(test)]
fn test_store() -> &'static Mutex<HashMap<String, String>> {
    static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct CredentialStore;

impl CredentialStore {
    #[cfg(test)]
    pub fn clear_all() {
        test_store().lock().unwrap().clear();
    }

    fn get_passphrase_impl(key_path: &str) -> Result<Option<String>, String> {
        #[cfg(test)]
        {
            return Ok(test_store().lock().unwrap().get(key_path).cloned());
        }
        #[cfg(not(test))]
        {
            match Entry::new(SERVICE_NAME, key_path) {
                Ok(entry) => match entry.get_password() {
                    Ok(p) => Ok(Some(p)),
                    Err(Error::NoEntry) => Ok(None),
                    Err(e) => Err(e.to_string()),
                },
                Err(e) => Err(e.to_string()),
            }
        }
    }

    fn set_passphrase_impl(key_path: &str, passphrase: &str) -> Result<(), String> {
        #[cfg(test)]
        {
            test_store().lock().unwrap().insert(key_path.to_string(), passphrase.to_string());
            return Ok(());
        }
        #[cfg(not(test))]
        {
            Entry::new(SERVICE_NAME, key_path)
                .map_err(|e| e.to_string())?
                .set_password(passphrase)
                .map_err(|e| e.to_string())
        }
    }

    fn delete_passphrase_impl(key_path: &str) -> Result<(), String> {
        #[cfg(test)]
        {
            test_store().lock().unwrap().remove(key_path);
            return Ok(());
        }
        #[cfg(not(test))]
        {
            match Entry::new(SERVICE_NAME, key_path) {
                Ok(entry) => match entry.delete_credential() {
                    Ok(()) | Err(Error::NoEntry) => Ok(()),
                    Err(e) => Err(e.to_string()),
                },
                Err(_) => Ok(()),
            }
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

//! Secret storage. Production uses the OS credential store (Windows Credential Manager
//! via `keyring`). Tests use an in-memory store. Secrets never reach the frontend.
use crate::error::{AppError, AppResult};
use std::collections::HashMap;
use std::sync::Mutex;

const SERVICE: &str = "io.github.socialeye";

pub trait SecretStore: Send + Sync {
    fn set(&self, key: &str, value: &str) -> AppResult<()>;
    fn get(&self, key: &str) -> AppResult<Option<String>>;
    fn delete(&self, key: &str) -> AppResult<()>;
}

pub struct KeyringStore;

impl SecretStore for KeyringStore {
    fn set(&self, key: &str, value: &str) -> AppResult<()> {
        Ok(keyring::Entry::new(SERVICE, key)?.set_password(value)?)
    }
    fn get(&self, key: &str) -> AppResult<Option<String>> {
        match keyring::Entry::new(SERVICE, key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    fn delete(&self, key: &str) -> AppResult<()> {
        match keyring::Entry::new(SERVICE, key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn set(&self, key: &str, value: &str) -> AppResult<()> {
        self.0.lock().map_err(|_| AppError::Secret { message: "poisoned".into() })?.insert(key.into(), value.into());
        Ok(())
    }
    fn get(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self.0.lock().map_err(|_| AppError::Secret { message: "poisoned".into() })?.get(key).cloned())
    }
    fn delete(&self, key: &str) -> AppResult<()> {
        self.0.lock().map_err(|_| AppError::Secret { message: "poisoned".into() })?.remove(key);
        Ok(())
    }
}

/// Key naming helpers so every module stores secrets consistently.
pub fn token_key(account_id: i64) -> String {
    format!("account:{account_id}:token")
}
pub fn client_secret_key(platform: &str) -> String {
    format!("app:{platform}:client_secret")
}

/// Token bundle as stored (JSON) in the credential store.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix seconds.
    pub expires_at: Option<i64>,
}

pub fn save_token(store: &dyn SecretStore, account_id: i64, t: &TokenSet) -> AppResult<()> {
    store.set(&token_key(account_id), &serde_json::to_string(t)?)
}

pub fn load_token(store: &dyn SecretStore, account_id: i64) -> AppResult<Option<TokenSet>> {
    match store.get(&token_key(account_id))? {
        Some(s) => Ok(Some(serde_json::from_str(&s)?)),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_roundtrip_and_delete() {
        let s = MemoryStore::default();
        let t = TokenSet { access_token: "a".into(), refresh_token: Some("r".into()), expires_at: Some(10) };
        save_token(&s, 7, &t).unwrap();
        assert_eq!(load_token(&s, 7).unwrap(), Some(t));
        s.delete(&token_key(7)).unwrap();
        assert_eq!(load_token(&s, 7).unwrap(), None);
    }
}

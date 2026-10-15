//! Encrypt secrets at rest with AES-256-GCM.
//! Key material is derived from `secrets.master_key` (SHA-256).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::sync::{Arc, RwLock};
use thiserror::Error;

const NONCE_LEN: usize = 12;

#[derive(Debug, Error)]
pub enum SecretStoreError {
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed")]
    Decrypt,
    #[error("invalid ciphertext")]
    InvalidCiphertext,
}

#[derive(Clone)]
pub struct SecretStore {
    /// SHA-256 of the master key. Clones share it so a rotation updates every user.
    key: Arc<RwLock<[u8; 32]>>,
}

/// Serializes encrypt, decrypt, and rotation inside this process.
pub fn operations_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    &LOCK
}

fn key_bytes(master_key: &str) -> [u8; 32] {
    let digest = Sha256::digest(master_key.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

impl SecretStore {
    pub fn from_master_key(master_key: &str) -> Self {
        Self {
            key: Arc::new(RwLock::new(key_bytes(master_key))),
        }
    }

    /// Swaps the key used by this store and every clone of it.
    pub fn replace_master_key(&self, master_key: &str) {
        let mut guard = self.key.write().unwrap_or_else(|err| err.into_inner());
        *guard = key_bytes(master_key);
    }

    fn cipher(&self) -> Aes256Gcm {
        let guard = self.key.read().unwrap_or_else(|err| err.into_inner());
        let key = Key::<Aes256Gcm>::try_from(guard.as_slice()).expect("sha256 is 32 bytes");
        Aes256Gcm::new(&key)
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<(Vec<u8>, Vec<u8>), SecretStoreError> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::try_from(&nonce_bytes[..]).map_err(|_| SecretStoreError::Encrypt)?;
        let ciphertext = self
            .cipher()
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|_| SecretStoreError::Encrypt)?;
        Ok((ciphertext, nonce_bytes.to_vec()))
    }

    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<String, SecretStoreError> {
        let nonce = Nonce::try_from(nonce).map_err(|_| SecretStoreError::InvalidCiphertext)?;
        let plain = self
            .cipher()
            .decrypt(&nonce, ciphertext)
            .map_err(|_| SecretStoreError::Decrypt)?;
        String::from_utf8(plain).map_err(|_| SecretStoreError::InvalidCiphertext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let store = SecretStore::from_master_key("test-master-key");
        let (ct, nonce) = store.encrypt("ghp_secret").unwrap();
        assert_eq!(store.decrypt(&ct, &nonce).unwrap(), "ghp_secret");
    }

    #[test]
    fn replace_master_key_updates_clones() {
        let store = SecretStore::from_master_key("old-master-key");
        let clone = store.clone();
        let (ct, nonce) = store.encrypt("forge-token").unwrap();
        store.replace_master_key("new-master-key");
        assert!(store.decrypt(&ct, &nonce).is_err());
        assert!(clone.decrypt(&ct, &nonce).is_err());
        let (next_ct, next_nonce) = clone.encrypt("forge-token").unwrap();
        assert_eq!(store.decrypt(&next_ct, &next_nonce).unwrap(), "forge-token");
    }
}

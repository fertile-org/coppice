//! Encrypt secrets at rest with AES-256-GCM.
//! Key material is derived from `secrets.master_key` (SHA-256).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use rand::RngCore;
use sha2::{Digest, Sha256};
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
    cipher: Aes256Gcm,
}

impl SecretStore {
    pub fn from_master_key(master_key: &str) -> Self {
        let digest = Sha256::digest(master_key.as_bytes());
        let key = Key::<Aes256Gcm>::try_from(digest.as_slice()).expect("sha256 is 32 bytes");
        Self {
            cipher: Aes256Gcm::new(&key),
        }
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<(Vec<u8>, Vec<u8>), SecretStoreError> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::try_from(&nonce_bytes[..]).map_err(|_| SecretStoreError::Encrypt)?;
        let ciphertext = self
            .cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|_| SecretStoreError::Encrypt)?;
        Ok((ciphertext, nonce_bytes.to_vec()))
    }

    pub fn decrypt(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<String, SecretStoreError> {
        let nonce = Nonce::try_from(nonce).map_err(|_| SecretStoreError::InvalidCiphertext)?;
        let plain = self
            .cipher
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
}

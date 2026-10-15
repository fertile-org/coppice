//! Desktop rotation of `session_secret` and `secrets.master_key`.
//!
//! Neither value could be rotated before: both are created once under
//! `secrets/` and reused. `session_secret` is not what session cookies are
//! checked against (those are random tokens in `sessions`), so rotation
//! deletes every session. `master_key` is re-applied by decrypting each
//! `secrets` row with the old key and writing the new ciphertext in one
//! transaction, then swapping the key file.
//!
//! A crash after that commit leaves `*.pending` files. The next start (or the
//! next rotation) promotes them when the database matches the pending key.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use rand::RngCore;
use sqlx::Row;
use uuid::Uuid;

use crate::crypto::{SecretStore, SecretStoreError};

const SECRET_BYTES: usize = 32;
const MASTER_FILE: &str = "master_key";
const SESSION_FILE: &str = "session_secret";
const MASTER_PENDING: &str = "master_key.pending";
const SESSION_PENDING: &str = "session_secret.pending";

pub const ROTATE_DESKTOP_ONLY: &str =
    "Secret rotation is only available in the desktop app.";
pub const ROTATED_MESSAGE: &str =
    "Secrets rotated. Saved tokens were re-encrypted. Sign in again.";

#[derive(Debug, thiserror::Error)]
pub enum RotateError {
    #[error("{ROTATE_DESKTOP_ONLY}")]
    NotDesktop,
    #[error("Saved tokens could not be re-encrypted, so the encryption key was not changed.")]
    Decrypt,
    #[error("A desktop secret file is unreadable, so secrets were not changed.")]
    Corrupt,
    #[error("Saved tokens cannot be decrypted with the current encryption key or the pending replacement.")]
    Unreadable,
    #[error("Secrets could not be rotated: {0}")]
    Io(#[from] io::Error),
    #[error("Secrets could not be rotated: {0}")]
    Database(#[from] sqlx::Error),
}

/// `data-dir/secrets` when this process is the desktop app and that folder exists.
pub fn desktop_secrets_dir(desktop_mode: bool, config_path: Option<&Path>) -> Option<PathBuf> {
    if !desktop_mode {
        return None;
    }
    let parent = config_path?.parent()?;
    let dir = parent.join("secrets");
    dir.is_dir().then_some(dir)
}

pub fn read_session_secret(dir: &Path) -> Option<String> {
    read_secret(&dir.join(SESSION_FILE)).ok()
}

/// Finish a rotation that committed in the database but did not swap the files.
/// Returns the master key that should be used when a pending key was promoted.
pub async fn recover_pending(
    pool: &sqlx::PgPool,
    dir: &Path,
) -> Result<Option<String>, RotateError> {
    let _guard = crate::crypto::operations_lock().lock().await;
    recover_pending_locked(pool, dir).await
}

/// Generate new desktop secrets, re-encrypt every saved token, and sign everyone out.
pub async fn rotate_desktop_secrets(
    pool: &sqlx::PgPool,
    dir: &Path,
    store: &SecretStore,
) -> Result<(), RotateError> {
    let _guard = crate::crypto::operations_lock().lock().await;
    if !dir.join(MASTER_FILE).is_file() || !dir.join(SESSION_FILE).is_file() {
        return Err(RotateError::NotDesktop);
    }
    if let Some(key) = recover_pending_locked(pool, dir).await? {
        store.replace_master_key(&key);
        return Ok(());
    }

    let old_master = read_secret(&dir.join(MASTER_FILE))?;
    let _old_session = read_secret(&dir.join(SESSION_FILE))?;
    let new_master = generate_secret();
    let new_session = generate_secret();
    write_secret_atomic(&dir.join(MASTER_PENDING), &new_master)?;
    write_secret_atomic(&dir.join(SESSION_PENDING), &new_session)?;

    let old_store = SecretStore::from_master_key(&old_master);
    let new_store = SecretStore::from_master_key(&new_master);
    if let Err(err) = reencrypt_and_sign_out(pool, &old_store, &new_store).await {
        let _ = std::fs::remove_file(dir.join(MASTER_PENDING));
        let _ = std::fs::remove_file(dir.join(SESSION_PENDING));
        return Err(err);
    }

    store.replace_master_key(&new_master);
    if let Err(err) = promote_pending(dir) {
        tracing::error!(
            error = %err,
            "secret rotation committed but the new key files were not moved into place"
        );
        return Err(err);
    }
    Ok(())
}

async fn recover_pending_locked(
    pool: &sqlx::PgPool,
    dir: &Path,
) -> Result<Option<String>, RotateError> {
    let master_pending = dir.join(MASTER_PENDING);
    let session_pending = dir.join(SESSION_PENDING);
    if !master_pending.is_file() && !session_pending.is_file() {
        return Ok(None);
    }

    if master_pending.is_file() {
        let current = read_secret(&dir.join(MASTER_FILE))?;
        let pending = read_secret(&master_pending)?;
        match pending_key_state(pool, &current, &pending).await? {
            PendingKey::Unused => {
                let _ = std::fs::remove_file(&master_pending);
                let _ = std::fs::remove_file(&session_pending);
                return Ok(None);
            }
            PendingKey::Committed => {
                promote_pending(dir)?;
                return Ok(Some(pending));
            }
        }
    }

    if session_pending.is_file() {
        std::fs::rename(&session_pending, dir.join(SESSION_FILE))?;
    }
    Ok(None)
}

enum PendingKey {
    Unused,
    Committed,
}

async fn pending_key_state(
    pool: &sqlx::PgPool,
    current: &str,
    pending: &str,
) -> Result<PendingKey, RotateError> {
    let row = sqlx::query("SELECT ciphertext, nonce FROM secrets LIMIT 1")
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        // No ciphertext to strand. Installing the pending key is safe.
        return Ok(PendingKey::Committed);
    };
    let ciphertext: Vec<u8> = row.get("ciphertext");
    let nonce: Vec<u8> = row.get("nonce");
    let old = SecretStore::from_master_key(current);
    if old.decrypt(&ciphertext, &nonce).is_ok() {
        return Ok(PendingKey::Unused);
    }
    let new = SecretStore::from_master_key(pending);
    if new.decrypt(&ciphertext, &nonce).is_ok() {
        return Ok(PendingKey::Committed);
    }
    Err(RotateError::Unreadable)
}

async fn reencrypt_and_sign_out(
    pool: &sqlx::PgPool,
    old: &SecretStore,
    new: &SecretStore,
) -> Result<(), RotateError> {
    let mut tx = pool.begin().await?;
    let rows = sqlx::query("SELECT id, ciphertext, nonce FROM secrets FOR UPDATE")
        .fetch_all(&mut *tx)
        .await?;
    for row in rows {
        let id: Uuid = row.get("id");
        let ciphertext: Vec<u8> = row.get("ciphertext");
        let nonce: Vec<u8> = row.get("nonce");
        let plain = old
            .decrypt(&ciphertext, &nonce)
            .map_err(map_decrypt)?;
        let (next_ciphertext, next_nonce) = new.encrypt(&plain).map_err(map_decrypt)?;
        sqlx::query(
            "UPDATE secrets SET ciphertext = $2, nonce = $3, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(&next_ciphertext)
        .bind(&next_nonce)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM sessions")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

fn map_decrypt(err: SecretStoreError) -> RotateError {
    match err {
        SecretStoreError::Decrypt | SecretStoreError::InvalidCiphertext => RotateError::Decrypt,
        SecretStoreError::Encrypt => RotateError::Decrypt,
    }
}

fn promote_pending(dir: &Path) -> Result<(), RotateError> {
    let master_pending = dir.join(MASTER_PENDING);
    if master_pending.is_file() {
        std::fs::rename(&master_pending, dir.join(MASTER_FILE))?;
    }
    let session_pending = dir.join(SESSION_PENDING);
    if session_pending.is_file() {
        std::fs::rename(&session_pending, dir.join(SESSION_FILE))?;
    }
    if let Ok(file) = std::fs::File::open(dir) {
        let _ = file.sync_all();
    }
    Ok(())
}

fn generate_secret() -> String {
    let mut bytes = [0u8; SECRET_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn read_secret(path: &Path) -> Result<String, RotateError> {
    let value = std::fs::read_to_string(path)
        .map_err(RotateError::Io)?
        .trim()
        .to_string();
    if value.len() != SECRET_BYTES * 2 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(RotateError::Corrupt);
    }
    Ok(value)
}

fn write_secret_atomic(path: &Path, value: &str) -> Result<(), RotateError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let tmp = path.with_extension("writing");
    {
        let mut file = std::fs::File::create(&tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(value.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(all(test, feature = "embedded-test-db"))]
mod tests {
    use super::*;
    use crate::crypto::SecretStore;
    use sqlx::Row;
    use uuid::Uuid;

    async fn pool() -> sqlx::PgPool {
        let pool = crate::db::shared_test_pool()
            .await
            .expect("embedded test database");
        sqlx::query("DELETE FROM sessions")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM secrets")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn secrets_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let secrets = dir.path().join("secrets");
        std::fs::create_dir_all(&secrets).unwrap();
        dir
    }

    fn write_pair(dir: &Path, master: &str, session: &str) {
        write_secret_atomic(&dir.join(MASTER_FILE), master).unwrap();
        write_secret_atomic(&dir.join(SESSION_FILE), session).unwrap();
    }

    async fn insert_secret(pool: &sqlx::PgPool, store: &SecretStore, plain: &str) {
        let (ciphertext, nonce) = store.encrypt(plain).unwrap();
        sqlx::query(
            "INSERT INTO secrets (id, name, ciphertext, nonce) VALUES ($1, $2, $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind("forge")
        .bind(&ciphertext)
        .bind(&nonce)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_session(pool: &sqlx::PgPool) {
        let user_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, role) VALUES ($1, $2, 'x', 'admin')",
        )
        .bind(user_id)
        .bind(format!("{user_id}@localhost"))
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, user_id, token_hash, csrf_token, expires_at) \
             VALUES ($1, $2, $3, 'csrf', now() + interval '1 day')",
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(format!("token-{user_id}"))
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn rotation_reencrypts_tokens_and_clears_sessions() {
        let pool = pool().await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        let old_master = generate_secret();
        let old_session = generate_secret();
        write_pair(&dir, &old_master, &old_session);
        let old_store = SecretStore::from_master_key(&old_master);
        insert_secret(&pool, &old_store, "forge-token-plain").await;
        insert_session(&pool).await;
        let live = SecretStore::from_master_key(&old_master);

        rotate_desktop_secrets(&pool, &dir, &live)
            .await
            .expect("rotate");

        let new_master = read_secret(&dir.join(MASTER_FILE)).unwrap();
        let new_session = read_secret(&dir.join(SESSION_FILE)).unwrap();
        assert_ne!(new_master, old_master);
        assert_ne!(new_session, old_session);
        assert!(!dir.join(MASTER_PENDING).exists());
        assert!(!dir.join(SESSION_PENDING).exists());

        let row = sqlx::query("SELECT ciphertext, nonce FROM secrets")
            .fetch_one(&pool)
            .await
            .unwrap();
        let ciphertext: Vec<u8> = row.get("ciphertext");
        let nonce: Vec<u8> = row.get("nonce");
        assert!(old_store.decrypt(&ciphertext, &nonce).is_err());
        assert_eq!(
            live.decrypt(&ciphertext, &nonce).unwrap(),
            "forge-token-plain"
        );
        let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(sessions, 0);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.join(MASTER_FILE)).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[tokio::test]
    async fn pending_key_is_promoted_when_the_database_uses_it() {
        let pool = pool().await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        let old_master = generate_secret();
        let new_master = generate_secret();
        write_pair(&dir, &old_master, &generate_secret());
        write_secret_atomic(&dir.join(MASTER_PENDING), &new_master).unwrap();
        write_secret_atomic(&dir.join(SESSION_PENDING), &generate_secret()).unwrap();
        let new_store = SecretStore::from_master_key(&new_master);
        insert_secret(&pool, &new_store, "already-new").await;

        let promoted = recover_pending(&pool, &dir).await.expect("recover");
        assert_eq!(promoted.as_deref(), Some(new_master.as_str()));
        assert_eq!(read_secret(&dir.join(MASTER_FILE)).unwrap(), new_master);
        assert!(!dir.join(MASTER_PENDING).exists());
        assert!(!dir.join(SESSION_PENDING).exists());
    }

    #[tokio::test]
    async fn pending_key_is_discarded_when_the_database_still_uses_the_old_one() {
        let pool = pool().await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        let old_master = generate_secret();
        write_pair(&dir, &old_master, &generate_secret());
        write_secret_atomic(&dir.join(MASTER_PENDING), &generate_secret()).unwrap();
        let old_store = SecretStore::from_master_key(&old_master);
        insert_secret(&pool, &old_store, "still-old").await;

        let promoted = recover_pending(&pool, &dir).await.expect("recover");
        assert_eq!(promoted, None);
        assert_eq!(read_secret(&dir.join(MASTER_FILE)).unwrap(), old_master);
        assert!(!dir.join(MASTER_PENDING).exists());
    }
}

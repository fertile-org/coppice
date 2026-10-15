//! Replace desktop and server keys: `session_secret`, `secrets.master_key`,
//! the admin password, and the Postgres role password.
//!
//! `session_secret` is not what session cookies are checked against (those are
//! random tokens in `sessions`), so replacement deletes every session.
//! `master_key` is re-applied by decrypting each `secrets` row with the old
//! key and writing the new ciphertext in one transaction, together with the
//! new admin password hash.
//!
//! The Postgres password cannot share that transaction. The new password is
//! written to `pg_password.pending` first, then `ALTER ROLE`, then the pending
//! file is promoted. A crash in either gap is finished on the next launch by
//! trying the current password and then the pending one.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use rand::RngCore;
use sqlx::postgres::PgConnectOptions;
use sqlx::{ConnectOptions, Connection, Row};
use uuid::Uuid;

use crate::crypto::{SecretStore, SecretStoreError};
use crate::services::auth_service::{hash_password, verify_password, MIN_PASSWORD_CHARS};

const SECRET_BYTES: usize = 32;
const MASTER_FILE: &str = "master_key";
const SESSION_FILE: &str = "session_secret";
const ADMIN_FILE: &str = "admin_password";
const PG_FILE: &str = "pg_password";
const MASTER_PENDING: &str = "master_key.pending";
const SESSION_PENDING: &str = "session_secret.pending";
const ADMIN_PENDING: &str = "admin_password.pending";
pub const PG_PENDING: &str = "pg_password.pending";

pub const REPLACE_UI_ERROR: &str = "Couldn't replace keys. Nothing was changed.";
pub const REPLACE_ON_SERVER: &str =
    "On a server, stop Coppice and run `coppice-server replace-keys`.";
pub const PASSWORD_EMPTY: &str = "Enter a new admin password.";

#[derive(Debug, thiserror::Error)]
pub enum ReplaceError {
    #[error("{REPLACE_ON_SERVER}")]
    NotDesktop,
    #[error("{PASSWORD_EMPTY}")]
    PasswordEmpty,
    #[error("Use at least {MIN_PASSWORD_CHARS} characters.")]
    PasswordShort,
    /// Logged, not shown in the app. The UI always says [`REPLACE_UI_ERROR`].
    #[error("{0}")]
    Failed(String),
}

impl From<io::Error> for ReplaceError {
    fn from(err: io::Error) -> Self {
        Self::Failed(err.to_string())
    }
}

impl From<sqlx::Error> for ReplaceError {
    fn from(err: sqlx::Error) -> Self {
        Self::Failed(err.to_string())
    }
}

pub fn check_admin_password(password: &str) -> Result<(), ReplaceError> {
    if password.trim().is_empty() {
        return Err(ReplaceError::PasswordEmpty);
    }
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(ReplaceError::PasswordShort);
    }
    Ok(())
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

/// Finish a replacement that committed in the database but did not swap the files.
/// Returns the master key that should be used when a pending key was promoted.
pub async fn recover_pending(
    pool: &sqlx::PgPool,
    dir: &Path,
) -> Result<Option<String>, ReplaceError> {
    let _guard = crate::crypto::operations_lock().lock().await;
    recover_pending_locked(pool, dir).await
}

/// Try the current `pg_password` file, then `pg_password.pending`.
/// No pending file returns the current password without connecting.
pub async fn finish_file_password(
    dir: &Path,
    host: &str,
    port: u16,
    user: &str,
    database: &str,
) -> Result<String, ReplaceError> {
    let current = read_secret(&dir.join(PG_FILE))?;
    let pending_path = dir.join(PG_PENDING);
    if !pending_path.is_file() {
        return Ok(current);
    }
    if password_connects(host, port, user, &current, database).await {
        let _ = std::fs::remove_file(&pending_path);
        return Ok(current);
    }
    let pending = read_secret(&pending_path)?;
    if password_connects(host, port, user, &pending, database).await {
        std::fs::rename(&pending_path, dir.join(PG_FILE))?;
        return Ok(pending);
    }
    Err(ReplaceError::Failed(
        "the current postgres password and the pending replacement both failed to connect"
            .into(),
    ))
}

/// Server installs keep the postgres password in `database.url`. A pending
/// file beside the config is the crash record.
pub async fn finish_server_pg_password(
    config_path: &Path,
    url: &str,
) -> Result<String, ReplaceError> {
    let Some(dir) = config_path.parent() else {
        return Ok(url.to_string());
    };
    let pending_path = dir.join(PG_PENDING);
    if !pending_path.is_file() {
        return Ok(url.to_string());
    }
    let target = db_target(url)?;
    if password_connects(
        &target.host,
        target.port,
        &target.user,
        &target.password,
        &target.database,
    )
    .await
    {
        let _ = std::fs::remove_file(&pending_path);
        return Ok(url.to_string());
    }
    let pending = read_secret(&pending_path)?;
    if password_connects(
        &target.host,
        target.port,
        &target.user,
        &pending,
        &target.database,
    )
    .await
    {
        let new_url = replace_url_password(url, &pending)?;
        set_config_values(config_path, &[(&["database", "url"], new_url.as_str())])?;
        let _ = std::fs::remove_file(&pending_path);
        return Ok(new_url);
    }
    Err(ReplaceError::Failed(
        "the database url password and the pending replacement both failed to connect".into(),
    ))
}

/// Master key, session secret, and admin password after a server crash.
pub struct RecoveredServerKeys {
    pub master_key: String,
    pub session_secret: String,
    pub admin_password: Option<String>,
}

/// Finish key files left beside a server `config.toml`. The Postgres password
/// is settled separately, before connecting, by [`finish_server_pg_password`].
pub async fn finish_server_keys(
    pool: &sqlx::PgPool,
    config_path: &Path,
    master_key: &str,
    session_secret: &str,
) -> Result<RecoveredServerKeys, ReplaceError> {
    let _guard = crate::crypto::operations_lock().lock().await;
    finish_server_keys_locked(pool, config_path, master_key, session_secret).await
}

/// Desktop Replace keys. Returns the database URL with the new password.
/// The pool's future connections use that password before this returns.
pub async fn replace_desktop_keys(
    pool: &sqlx::PgPool,
    store: &SecretStore,
    dir: &Path,
    database_url: &str,
    admin_password: &str,
    admin_email: &str,
) -> Result<String, ReplaceError> {
    let _guard = crate::crypto::operations_lock().lock().await;
    if !dir.join(MASTER_FILE).is_file() || !dir.join(SESSION_FILE).is_file() {
        return Err(ReplaceError::NotDesktop);
    }
    check_admin_password(admin_password)?;
    ensure_admin(pool, admin_email).await?;
    let database_url = settle_desktop_pg(pool, dir, database_url).await?;
    if let Some(key) = recover_pending_locked(pool, dir).await? {
        store.replace_master_key(&key);
    }

    let old_master = read_secret(&dir.join(MASTER_FILE))?;
    let _old_session = read_secret(&dir.join(SESSION_FILE))?;
    let new_master = generate_secret();
    let new_session = generate_secret();
    let new_pg = generate_secret();
    let new_url = replace_url_password(&database_url, &new_pg)?;
    let password_hash = hash_password(admin_password)
        .map_err(|err| ReplaceError::Failed(format!("could not hash the admin password: {err}")))?;

    write_secret_atomic(&dir.join(PG_PENDING), &new_pg)?;
    let old_options = (*pool.connect_options()).clone();
    let outcome = apply_pg_then_keys(
        pool,
        dir,
        &old_options,
        &database_url,
        &new_pg,
        &new_url,
        &old_master,
        &new_master,
        &new_session,
        admin_password,
        admin_email,
        &password_hash,
        true,
    )
    .await;
    match outcome {
        Ok(()) => {
            store.replace_master_key(&new_master);
            Ok(new_url)
        }
        Err(err) => Err(err),
    }
}

/// Server Replace keys, with Coppice stopped and PostgreSQL still running.
/// Writes the new values into `config_path` and returns the new database URL.
#[allow(clippy::too_many_arguments)]
pub async fn replace_server_keys(
    pool: &sqlx::PgPool,
    store: &SecretStore,
    config_path: &Path,
    database_url: &str,
    master_key: &str,
    _session_secret: &str,
    admin_password: &str,
    admin_email: &str,
) -> Result<String, ReplaceError> {
    let _guard = crate::crypto::operations_lock().lock().await;
    check_admin_password(admin_password)?;
    ensure_admin(pool, admin_email).await?;
    let dir = config_path.parent().ok_or_else(|| {
        ReplaceError::Failed(format!("{} has no parent", config_path.display()))
    })?;
    let database_url = finish_server_pg_password(config_path, database_url).await?;
    adopt_url_password(pool, &database_url);
    let recovered =
        finish_server_keys_locked(pool, config_path, master_key, _session_secret).await?;
    if recovered.master_key != master_key {
        store.replace_master_key(&recovered.master_key);
    }
    let master_key = recovered.master_key;

    let new_master = generate_secret();
    let new_session = generate_secret();
    let new_pg = generate_secret();
    let new_url = replace_url_password(&database_url, &new_pg)?;
    let password_hash = hash_password(admin_password)
        .map_err(|err| ReplaceError::Failed(format!("could not hash the admin password: {err}")))?;

    write_secret_atomic(&dir.join(PG_PENDING), &new_pg)?;
    let old_options = (*pool.connect_options()).clone();
    apply_pg_then_keys(
        pool,
        dir,
        &old_options,
        &database_url,
        &new_pg,
        &new_url,
        &master_key,
        &new_master,
        &new_session,
        admin_password,
        admin_email,
        &password_hash,
        false,
    )
    .await?;
    set_config_values(
        config_path,
        &[
            (&["database", "url"], new_url.as_str()),
            (&["secrets", "master_key"], new_master.as_str()),
            (&["auth", "session_secret"], new_session.as_str()),
            (
                &["auth", "bootstrap_admin_password"],
                admin_password,
            ),
        ],
    )?;
    for name in [PG_PENDING, MASTER_PENDING, SESSION_PENDING, ADMIN_PENDING] {
        let _ = std::fs::remove_file(dir.join(name));
    }
    store.replace_master_key(&new_master);
    Ok(new_url)
}

#[allow(clippy::too_many_arguments)]
async fn apply_pg_then_keys(
    pool: &sqlx::PgPool,
    dir: &Path,
    old_options: &PgConnectOptions,
    database_url: &str,
    new_pg: &str,
    new_url: &str,
    old_master: &str,
    new_master: &str,
    new_session: &str,
    admin_password: &str,
    admin_email: &str,
    password_hash: &str,
    secret_files: bool,
) -> Result<(), ReplaceError> {
    let target = db_target(database_url)?;
    let mut conn = pool.acquire().await?;
    if let Err(err) = alter_role_password(&mut conn, &target.user, new_pg).await {
        let _ = std::fs::remove_file(dir.join(PG_PENDING));
        return Err(err);
    }
    if !password_connects(
        &target.host,
        target.port,
        &target.user,
        new_pg,
        &target.database,
    )
    .await
    {
        let _ = alter_role_password(&mut conn, &target.user, &target.password).await;
        let _ = std::fs::remove_file(dir.join(PG_PENDING));
        return Err(ReplaceError::Failed(
            "postgres did not accept the new password, so the role password was restored".into(),
        ));
    }
    pool.set_connect_options(old_options.clone().password(new_pg));

    write_secret_atomic(&dir.join(MASTER_PENDING), new_master)?;
    write_secret_atomic(&dir.join(SESSION_PENDING), new_session)?;
    write_password_atomic(&dir.join(ADMIN_PENDING), admin_password)?;

    let old_store = SecretStore::from_master_key(old_master);
    let new_store = SecretStore::from_master_key(new_master);
    let rotated = reencrypt_admin_and_sign_out(
        &mut conn,
        &old_store,
        &new_store,
        admin_email,
        password_hash,
    )
    .await;
    if let Err(err) = rotated {
        let _ = std::fs::remove_file(dir.join(MASTER_PENDING));
        let _ = std::fs::remove_file(dir.join(SESSION_PENDING));
        let _ = std::fs::remove_file(dir.join(ADMIN_PENDING));
        if let Err(revert) = alter_role_password(&mut conn, &target.user, &target.password).await {
            tracing::error!(
                error = %revert,
                "new postgres password could not be restored; the pending file was kept"
            );
        } else {
            pool.set_connect_options(old_options.clone());
            let _ = std::fs::remove_file(dir.join(PG_PENDING));
        }
        return Err(err);
    }
    drop(conn);

    if let Err(err) = promote_after_commit(dir, secret_files, new_url, new_master, new_session, admin_password)
    {
        tracing::error!(
            error = %err,
            "keys were replaced in the database but the files were not moved; the next launch finishes that"
        );
        return Err(err);
    }
    let _ = new_url;
    Ok(())
}

fn promote_after_commit(
    dir: &Path,
    secret_files: bool,
    _new_url: &str,
    _new_master: &str,
    _new_session: &str,
    _admin_password: &str,
) -> Result<(), ReplaceError> {
    if secret_files {
        promote_pending(dir)?;
    }
    Ok(())
}

async fn settle_desktop_pg(
    pool: &sqlx::PgPool,
    dir: &Path,
    database_url: &str,
) -> Result<String, ReplaceError> {
    if !dir.join(PG_PENDING).is_file() {
        return Ok(database_url.to_string());
    }
    let target = db_target(database_url)?;
    let pg = finish_file_password(dir, &target.host, target.port, &target.user, &target.database)
        .await?;
    if pg != target.password {
        adopt_url_password(pool, &replace_url_password(database_url, &pg)?);
        return replace_url_password(database_url, &pg);
    }
    Ok(database_url.to_string())
}

fn adopt_url_password(pool: &sqlx::PgPool, url: &str) {
    if let Ok(target) = db_target(url) {
        pool.set_connect_options((*pool.connect_options()).clone().password(&target.password));
    }
}

async fn ensure_admin(pool: &sqlx::PgPool, email: &str) -> Result<(), ReplaceError> {
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM users WHERE email = $1 AND role = 'admin' LIMIT 1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;
    if found.is_none() {
        return Err(ReplaceError::Failed(format!(
            "no admin user named {email}"
        )));
    }
    Ok(())
}

async fn alter_role_password(
    conn: &mut sqlx::PgConnection,
    role: &str,
    password: &str,
) -> Result<(), ReplaceError> {
    let sql = alter_role_sql(role, password)?;
    sqlx::query(&sql).execute(&mut *conn).await?;
    Ok(())
}

fn alter_role_sql(role: &str, password: &str) -> Result<String, ReplaceError> {
    if role.is_empty()
        || !role
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(ReplaceError::Failed(format!(
            "unsupported database role {role}"
        )));
    }
    if password.contains('\0') {
        return Err(ReplaceError::Failed(
            "password contains a null byte".into(),
        ));
    }
    Ok(format!(
        "ALTER ROLE {role} PASSWORD '{}'",
        password.replace('\'', "''")
    ))
}

async fn recover_pending_locked(
    pool: &sqlx::PgPool,
    dir: &Path,
) -> Result<Option<String>, ReplaceError> {
    let master_pending = dir.join(MASTER_PENDING);
    let session_pending = dir.join(SESSION_PENDING);
    if !master_pending.is_file() && !session_pending.is_file() && !dir.join(ADMIN_PENDING).is_file()
    {
        return Ok(None);
    }

    if master_pending.is_file() {
        let current = read_secret(&dir.join(MASTER_FILE))?;
        let pending = read_secret(&master_pending)?;
        match pending_key_state(pool, &current, &pending).await? {
            PendingKey::Unused => {
                let _ = std::fs::remove_file(&master_pending);
                let _ = std::fs::remove_file(&session_pending);
                let _ = std::fs::remove_file(dir.join(ADMIN_PENDING));
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
    settle_admin_file(pool, dir).await?;
    Ok(None)
}

async fn recover_master_value(
    pool: &sqlx::PgPool,
    current: &str,
    pending_path: &Path,
) -> Result<Option<String>, ReplaceError> {
    if !pending_path.is_file() {
        return Ok(None);
    }
    let pending = read_secret(pending_path)?;
    match pending_key_state(pool, current, &pending).await? {
        PendingKey::Unused => {
            let _ = std::fs::remove_file(pending_path);
            Ok(None)
        }
        PendingKey::Committed => Ok(Some(pending)),
    }
}

fn promote_server_config(
    config_path: &Path,
    dir: &Path,
    master: Option<&str>,
    session: Option<&str>,
    admin: Option<&str>,
    url: Option<&str>,
) -> Result<(), ReplaceError> {
    let mut pairs: Vec<(&[&str], &str)> = Vec::new();
    if let Some(master) = master {
        pairs.push((&["secrets", "master_key"], master));
    }
    if let Some(session) = session {
        pairs.push((&["auth", "session_secret"], session));
    }
    if let Some(admin) = admin {
        pairs.push((&["auth", "bootstrap_admin_password"], admin));
    }
    if let Some(url) = url {
        pairs.push((&["database", "url"], url));
    }
    if !pairs.is_empty() {
        set_config_values(config_path, &pairs)?;
    }
    for name in [MASTER_PENDING, SESSION_PENDING, ADMIN_PENDING] {
        let _ = std::fs::remove_file(dir.join(name));
    }
    Ok(())
}

async fn finish_server_keys_locked(
    pool: &sqlx::PgPool,
    config_path: &Path,
    master_key: &str,
    session_secret: &str,
) -> Result<RecoveredServerKeys, ReplaceError> {
    let unchanged = RecoveredServerKeys {
        master_key: master_key.to_string(),
        session_secret: session_secret.to_string(),
        admin_password: None,
    };
    let Some(dir) = config_path.parent() else {
        return Ok(unchanged);
    };
    if !dir.join(MASTER_PENDING).is_file()
        && !dir.join(SESSION_PENDING).is_file()
        && !dir.join(ADMIN_PENDING).is_file()
    {
        return Ok(unchanged);
    }

    let mut master = master_key.to_string();
    let mut session = session_secret.to_string();
    let mut admin_password = None;
    let mut write_master = false;
    let mut write_session = false;

    if dir.join(MASTER_PENDING).is_file() {
        match recover_master_value(pool, master_key, &dir.join(MASTER_PENDING)).await? {
            Some(key) => {
                master = key;
                write_master = true;
                if let Ok(value) = read_secret(&dir.join(SESSION_PENDING)) {
                    session = value;
                    write_session = true;
                }
            }
            None => {
                let _ = std::fs::remove_file(dir.join(SESSION_PENDING));
                let _ = std::fs::remove_file(dir.join(ADMIN_PENDING));
                return Ok(unchanged);
            }
        }
    } else if let Ok(value) = read_secret(&dir.join(SESSION_PENDING)) {
        session = value;
        write_session = true;
    }

    if dir.join(ADMIN_PENDING).is_file() {
        let pending = read_password_file(&dir.join(ADMIN_PENDING))?;
        match admin_hash(pool).await? {
            Some(hash) if password_matches(&pending, &hash) => {
                admin_password = Some(pending);
            }
            Some(_) if write_master => {
                return Err(ReplaceError::Failed(
                    "the pending admin password does not match the saved hash".into(),
                ));
            }
            _ => {
                let _ = std::fs::remove_file(dir.join(ADMIN_PENDING));
            }
        }
    }

    if write_master || write_session || admin_password.is_some() {
        promote_server_config(
            config_path,
            dir,
            write_master.then_some(master.as_str()),
            write_session.then_some(session.as_str()),
            admin_password.as_deref(),
            None,
        )?;
    }
    Ok(RecoveredServerKeys {
        master_key: master,
        session_secret: session,
        admin_password,
    })
}

enum PendingKey {
    Unused,
    Committed,
}

async fn pending_key_state(
    pool: &sqlx::PgPool,
    current: &str,
    pending: &str,
) -> Result<PendingKey, ReplaceError> {
    let row = sqlx::query(
        "SELECT ciphertext, nonce FROM secrets WHERE NOT needs_reentry LIMIT 1",
    )
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
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
    Err(ReplaceError::Failed(
        "saved tokens cannot be decrypted with the current encryption key or the pending replacement"
            .into(),
    ))
}

async fn reencrypt_admin_and_sign_out(
    conn: &mut sqlx::PgConnection,
    old: &SecretStore,
    new: &SecretStore,
    admin_email: &str,
    password_hash: &str,
) -> Result<(), ReplaceError> {
    let mut tx = conn.begin().await?;
    let rows = sqlx::query(
        "SELECT id, ciphertext, nonce, needs_reentry FROM secrets FOR UPDATE",
    )
    .fetch_all(&mut *tx)
    .await?;
    for row in rows {
        if row.get::<bool, _>("needs_reentry") {
            continue;
        }
        let id: Uuid = row.get("id");
        let ciphertext: Vec<u8> = row.get("ciphertext");
        let nonce: Vec<u8> = row.get("nonce");
        let plain = old.decrypt(&ciphertext, &nonce).map_err(map_decrypt)?;
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
    let updated = sqlx::query(
        "UPDATE users SET password_hash = $1 WHERE email = $2 AND role = 'admin'",
    )
    .bind(password_hash)
    .bind(admin_email)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(ReplaceError::Failed(format!(
            "no admin user named {admin_email}"
        )));
    }
    sqlx::query("DELETE FROM sessions")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

fn map_decrypt(err: SecretStoreError) -> ReplaceError {
    ReplaceError::Failed(match err {
        SecretStoreError::Decrypt | SecretStoreError::InvalidCiphertext => {
            "saved tokens could not be re-encrypted with the current key".into()
        }
        SecretStoreError::Encrypt => "saved tokens could not be re-encrypted".into(),
    })
}

async fn settle_admin_file(pool: &sqlx::PgPool, dir: &Path) -> Result<(), ReplaceError> {
    let pending_path = dir.join(ADMIN_PENDING);
    if !pending_path.is_file() {
        return Ok(());
    }
    let pending = read_password_file(&pending_path)?;
    let current_path = dir.join(ADMIN_FILE);
    let current = read_password_file(&current_path).ok();
    let hash = admin_hash(pool).await?;
    let Some(hash) = hash else {
        if current_path.is_file() {
            let _ = std::fs::remove_file(&pending_path);
        }
        return Ok(());
    };
    if current
        .as_deref()
        .is_some_and(|value| password_matches(value, &hash))
    {
        let _ = std::fs::remove_file(&pending_path);
        return Ok(());
    }
    if password_matches(&pending, &hash) {
        write_password_atomic(&current_path, &pending)?;
        let _ = std::fs::remove_file(&pending_path);
        return Ok(());
    }
    Err(ReplaceError::Failed(
        "the admin password file and its pending replacement do not match the saved hash".into(),
    ))
}

fn password_matches(password: &str, hash: &str) -> bool {
    verify_password(password, hash).unwrap_or(false)
}

async fn admin_hash(pool: &sqlx::PgPool) -> Result<Option<String>, ReplaceError> {
    let hash: Option<String> = sqlx::query_scalar(
        "SELECT password_hash FROM users WHERE role = 'admin' ORDER BY created_at ASC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;
    Ok(hash)
}

fn promote_pending(dir: &Path) -> Result<(), ReplaceError> {
    for (pending, current) in [
        (MASTER_PENDING, MASTER_FILE),
        (SESSION_PENDING, SESSION_FILE),
        (ADMIN_PENDING, ADMIN_FILE),
        (PG_PENDING, PG_FILE),
    ] {
        let pending_path = dir.join(pending);
        if pending_path.is_file() {
            std::fs::rename(&pending_path, dir.join(current))?;
        }
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

fn read_secret(path: &Path) -> Result<String, ReplaceError> {
    let value = std::fs::read_to_string(path)?.trim().to_string();
    if value.len() != SECRET_BYTES * 2 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ReplaceError::Failed(format!(
            "secret file {} is unreadable",
            path.display()
        )));
    }
    Ok(value)
}

fn read_password_file(path: &Path) -> Result<String, ReplaceError> {
    let value = std::fs::read_to_string(path)?.trim().to_string();
    if value.is_empty() || value.chars().any(|c| c.is_control()) {
        return Err(ReplaceError::Failed(format!(
            "secret file {} is unreadable",
            path.display()
        )));
    }
    Ok(value)
}

fn write_password_atomic(path: &Path, value: &str) -> Result<(), ReplaceError> {
    write_secret_atomic(path, value)
}

fn write_secret_atomic(path: &Path, value: &str) -> Result<(), ReplaceError> {
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

struct DbTarget {
    host: String,
    port: u16,
    user: String,
    password: String,
    database: String,
}

fn db_target(url: &str) -> Result<DbTarget, ReplaceError> {
    let opts = PgConnectOptions::from_str(url)
        .map_err(|err| ReplaceError::Failed(format!("could not parse the database url: {err}")))?;
    let password = password_from_url(url).filter(|value| !value.is_empty()).ok_or_else(|| {
        ReplaceError::Failed("database url has no password".into())
    })?;
    Ok(DbTarget {
        host: opts.get_host().to_string(),
        port: opts.get_port(),
        user: opts.get_username().to_string(),
        password,
        database: opts
            .get_database()
            .unwrap_or("postgres")
            .to_string(),
    })
}

fn password_from_url(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let (userinfo, _) = authority.rsplit_once('@')?;
    let (_, password) = userinfo.split_once(':')?;
    Some(percent_decode(password))
}

fn replace_url_password(url: &str, password: &str) -> Result<String, ReplaceError> {
    let (scheme, rest) = url.split_once("://").ok_or_else(|| {
        ReplaceError::Failed("database url has no scheme".into())
    })?;
    let auth_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..auth_end];
    let tail = &rest[auth_end..];
    let (userinfo, host) = authority.rsplit_once('@').ok_or_else(|| {
        ReplaceError::Failed("database url has no user".into())
    })?;
    let user = userinfo.split_once(':').map(|(user, _)| user).unwrap_or(userinfo);
    Ok(format!(
        "{scheme}://{user}:{}@{host}{tail}",
        percent_encode(password)
    ))
}

fn percent_encode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (byte as char).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or(""),
                16,
            ) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn set_config_values(path: &Path, values: &[(&[&str], &str)]) -> Result<(), ReplaceError> {
    let raw = if path.is_file() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    let mut doc = if raw.trim().is_empty() {
        toml_edit::DocumentMut::new()
    } else {
        raw.parse::<toml_edit::DocumentMut>()
            .map_err(|err| ReplaceError::Failed(err.to_string()))?
    };
    for (keys, value) in values {
        set_nested(doc.as_table_mut(), keys, value)?;
    }
    coppice_config::connector_file::write_toml_atomic(path, &doc.to_string())
        .map_err(|err| ReplaceError::Failed(err.to_string()))?;
    Ok(())
}

fn set_nested(table: &mut toml_edit::Table, keys: &[&str], value: &str) -> Result<(), ReplaceError> {
    let (first, rest) = keys
        .split_first()
        .ok_or_else(|| ReplaceError::Failed("empty config path".into()))?;
    if rest.is_empty() {
        table.insert(first, toml_edit::value(value));
        return Ok(());
    }
    let created = table.get(first).is_none();
    let item = table
        .entry(first)
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let child = item.as_table_mut().ok_or_else(|| {
        ReplaceError::Failed(format!("{first} is not a table in the config file"))
    })?;
    if created {
        child.set_implicit(false);
    }
    set_nested(child, rest, value)
}

async fn password_connects(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    database: &str,
) -> bool {
    let opts = PgConnectOptions::new()
        .host(host)
        .port(port)
        .username(user)
        .password(password)
        .database(database);
    match tokio::time::timeout(Duration::from_secs(4), opts.connect()).await {
        Ok(Ok(conn)) => {
            let _ = conn.close().await;
            true
        }
        _ => false,
    }
}

/// `coppice-server replace-keys --admin-password <value>`.
/// `Ok(None)` when `args` is a different command.
pub fn parse_replace_keys_password(args: &[String]) -> Result<Option<String>, String> {
    if args.get(1).map(String::as_str) != Some("replace-keys") {
        return Ok(None);
    }
    let mut password = None;
    let mut rest = args[2..].iter();
    while let Some(flag) = rest.next() {
        if flag == "--admin-password" {
            let value = rest
                .next()
                .ok_or_else(|| "--admin-password requires a value".to_string())?;
            password = Some(value.clone());
        } else {
            return Err(format!("unknown argument {flag}"));
        }
    }
    password
        .ok_or_else(|| "--admin-password is required".to_string())
        .map(Some)
}

/// Stop Coppice first and leave PostgreSQL running. Desktop installs keep the
/// database password in `secrets/` and change it from Tools → Backup.
pub async fn replace_offline(
    config: &crate::AppConfig,
    config_path: &Path,
    admin_password: &str,
) -> Result<String, ReplaceError> {
    let secrets = config_path.parent().map(|parent| parent.join("secrets"));
    if secrets
        .as_ref()
        .is_some_and(|dir| dir.join(MASTER_FILE).is_file() && dir.join(SESSION_FILE).is_file())
        && db_target(&config.database.url).is_err()
    {
        return Err(ReplaceError::Failed(
            "this data directory belongs to the desktop app. Use Tools → Backup → Replace keys while Coppice is open. coppice-server replace-keys is for a server install with PostgreSQL still running"
                .into(),
        ));
    }
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(8))
        .connect(&config.database.url)
        .await?;
    let store = SecretStore::from_master_key(&config.secrets.master_key);
    let email = config
        .auth
        .bootstrap_admin_email
        .clone()
        .unwrap_or_else(|| "admin@localhost".into());
    if let Some(dir) = secrets.filter(|dir| dir.join(MASTER_FILE).is_file()) {
        let _url = replace_desktop_keys(
            &pool,
            &store,
            &dir,
            &config.database.url,
            admin_password,
            &email,
        )
        .await?;
        return Ok(replaced_keys_message(&dir));
    }
    let _url = replace_server_keys(
        &pool,
        &store,
        config_path,
        &config.database.url,
        &config.secrets.master_key,
        &config.auth.session_secret,
        admin_password,
        &email,
    )
    .await?;
    Ok(replaced_keys_message(config_path))
}

pub fn replaced_keys_message(path: &Path) -> String {
    format!(
        "Keys replaced. The new values are in {}. Keep that file private.",
        path.display()
    )
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
        sqlx::query("DELETE FROM users")
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

    async fn insert_admin(pool: &sqlx::PgPool, email: &str, password: &str) {
        let hash = hash_password(password).unwrap();
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, role) VALUES ($1, $2, $3, 'admin')",
        )
        .bind(Uuid::new_v4())
        .bind(email)
        .bind(hash)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn insert_session(pool: &sqlx::PgPool) {
        let user_id: Uuid = sqlx::query_scalar("SELECT id FROM users LIMIT 1")
            .fetch_one(pool)
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

    #[test]
    fn password_messages_use_the_app_minimum() {
        assert_eq!(
            ReplaceError::PasswordShort.to_string(),
            format!("Use at least {MIN_PASSWORD_CHARS} characters.")
        );
        assert_eq!(MIN_PASSWORD_CHARS, 8);
        assert_eq!(
            check_admin_password("1234567").unwrap_err().to_string(),
            "Use at least 8 characters."
        );
        assert!(check_admin_password("").is_err());
        assert!(check_admin_password("   ").is_err());
        let ok = "a".repeat(MIN_PASSWORD_CHARS);
        assert!(check_admin_password(&ok).is_ok());
        assert_eq!(
            replaced_keys_message(Path::new("/tmp/config.toml")),
            "Keys replaced. The new values are in /tmp/config.toml. Keep that file private."
        );
    }

    #[tokio::test]
    async fn rotation_reencrypts_tokens_clears_sessions_and_sets_admin_password() {
        let pool = pool().await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        let old_master = generate_secret();
        let old_session = generate_secret();
        write_pair(&dir, &old_master, &old_session);
        write_password_atomic(&dir.join(ADMIN_FILE), "old-admin-pass").unwrap();
        write_secret_atomic(&dir.join(PG_FILE), &generate_secret()).unwrap();
        let old_store = SecretStore::from_master_key(&old_master);
        insert_secret(&pool, &old_store, "forge-token-plain").await;
        insert_admin(&pool, "admin@localhost", "old-admin-pass").await;
        insert_session(&pool).await;

        let new_master = generate_secret();
        let new_session = generate_secret();
        let hash = hash_password("new-admin-pass").unwrap();
        write_secret_atomic(&dir.join(MASTER_PENDING), &new_master).unwrap();
        write_secret_atomic(&dir.join(SESSION_PENDING), &new_session).unwrap();
        write_password_atomic(&dir.join(ADMIN_PENDING), "new-admin-pass").unwrap();
        let mut conn = pool.acquire().await.unwrap();
        reencrypt_admin_and_sign_out(
            &mut *conn,
            &old_store,
            &SecretStore::from_master_key(&new_master),
            "admin@localhost",
            &hash,
        )
        .await
        .unwrap();
        drop(conn);
        promote_pending(&dir).unwrap();

        let stored_hash: String =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE email = 'admin@localhost'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!verify_password("old-admin-pass", &stored_hash).unwrap());
        assert!(verify_password("new-admin-pass", &stored_hash).unwrap());
        assert_eq!(
            read_password_file(&dir.join(ADMIN_FILE)).unwrap(),
            "new-admin-pass"
        );
        let row = sqlx::query("SELECT ciphertext, nonce FROM secrets")
            .fetch_one(&pool)
            .await
            .unwrap();
        let ciphertext: Vec<u8> = row.get("ciphertext");
        let nonce: Vec<u8> = row.get("nonce");
        assert!(old_store.decrypt(&ciphertext, &nonce).is_err());
        assert_eq!(
            SecretStore::from_master_key(&new_master)
                .decrypt(&ciphertext, &nonce)
                .unwrap(),
            "forge-token-plain"
        );
        let sessions: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(sessions, 0);
    }

    #[tokio::test]
    async fn fresh_key_flags_the_token_and_replace_skips_it() {
        let pool = pool().await;
        insert_admin(&pool, "admin@localhost", "old-admin-pass").await;
        let old_store = SecretStore::from_master_key("old-master-key");
        let fresh = SecretStore::from_master_key("fresh-install-key");
        insert_secret(&pool, &old_store, "ghp_BROWSERTOKEN_9f3c1a").await;

        let flagged = crate::services::secret_service::SecretService::new(&pool, &fresh)
            .flag_undecryptable()
            .await
            .unwrap();
        assert_eq!(flagged, 1);
        let row = sqlx::query("SELECT ciphertext, needs_reentry FROM secrets")
            .fetch_one(&pool)
            .await
            .unwrap();
        let ciphertext: Vec<u8> = row.get("ciphertext");
        assert!(ciphertext.is_empty());
        assert!(row.get::<bool, _>("needs_reentry"));

        let mut conn = pool.acquire().await.unwrap();
        reencrypt_admin_and_sign_out(
            &mut *conn,
            &fresh,
            &SecretStore::from_master_key("replaced-key"),
            "admin@localhost",
            "hash",
        )
        .await
        .expect("replace skips the flagged token");
        drop(conn);

        let still: i64 =
            sqlx::query_scalar("SELECT count(*) FROM secrets WHERE needs_reentry")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(still, 1);
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

    struct RoleGuard {
        name: String,
        host: String,
        port: u16,
        database: String,
        admin: PgConnectOptions,
    }

    impl RoleGuard {
        async fn create(pool: &sqlx::PgPool, password: &str) -> Self {
            let opts = (*pool.connect_options()).clone();
            let name = format!("rk_{:x}", Uuid::new_v4().as_u128());
            let database = opts.get_database().unwrap().to_string();
            let create = format!(
                "CREATE ROLE {name} LOGIN PASSWORD '{}'",
                password.replace('\'', "''")
            );
            sqlx::query(&create).execute(pool).await.unwrap();
            sqlx::query(&format!(
                "GRANT CONNECT ON DATABASE \"{database}\" TO {name}"
            ))
            .execute(pool)
            .await
            .unwrap();
            Self {
                name,
                host: opts.get_host().to_string(),
                port: opts.get_port(),
                database,
                admin: opts,
            }
        }

        async fn drop_role(&self) {
            let Ok(mut conn) = self.admin.connect().await else {
                return;
            };
            let _ = sqlx::query(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE usename = $1",
            )
            .bind(&self.name)
            .execute(&mut conn)
            .await;
            let _ = sqlx::query(&format!("DROP ROLE IF EXISTS {}", self.name))
                .execute(&mut conn)
                .await;
            let _ = conn.close().await;
        }
    }

    /// Process death is the absence of in-memory state: the next launch only
    /// has the files and PostgreSQL.
    #[tokio::test]
    async fn crash_between_pending_write_and_alter_still_connects() {
        let pool = pool().await;
        let old_pg = generate_secret();
        let new_pg = generate_secret();
        let role = RoleGuard::create(&pool, &old_pg).await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        write_secret_atomic(&dir.join(PG_FILE), &old_pg).unwrap();
        write_secret_atomic(&dir.join(PG_PENDING), &new_pg).unwrap();

        let resolved = finish_file_password(&dir, &role.host, role.port, &role.name, &role.database)
            .await
            .expect("finish");
        assert_eq!(resolved, old_pg);
        assert!(!dir.join(PG_PENDING).exists());
        assert!(
            password_connects(&role.host, role.port, &role.name, &old_pg, &role.database).await
        );
        assert!(
            !password_connects(&role.host, role.port, &role.name, &new_pg, &role.database).await
        );
        role.drop_role().await;
    }

    #[tokio::test]
    async fn crash_between_alter_role_and_promote_still_connects() {
        let pool = pool().await;
        let old_pg = generate_secret();
        let new_pg = generate_secret();
        let role = RoleGuard::create(&pool, &old_pg).await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        write_secret_atomic(&dir.join(PG_FILE), &old_pg).unwrap();
        write_secret_atomic(&dir.join(PG_PENDING), &new_pg).unwrap();
        let mut conn = pool.acquire().await.unwrap();
        alter_role_password(&mut *conn, &role.name, &new_pg)
            .await
            .unwrap();
        drop(conn);

        let resolved = finish_file_password(&dir, &role.host, role.port, &role.name, &role.database)
            .await
            .expect("finish");
        assert_eq!(resolved, new_pg);
        assert_eq!(read_secret(&dir.join(PG_FILE)).unwrap(), new_pg);
        assert!(!dir.join(PG_PENDING).exists());
        assert!(
            password_connects(&role.host, role.port, &role.name, &new_pg, &role.database).await
        );
        assert!(
            !password_connects(&role.host, role.port, &role.name, &old_pg, &role.database).await
        );
        role.drop_role().await;
    }
}

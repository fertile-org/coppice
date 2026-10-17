//! Replace desktop keys: `session_secret`, `secrets.master_key`,
//! the admin password, and the Postgres role password.
//!
//! `session_secret` is not what session cookies are checked against (those are
//! random tokens in `sessions`), so replacement deletes every session.
//! `master_key` is re-applied by decrypting each `secrets` row with the old
//! key and writing the new ciphertext in one transaction, together with the
//! new admin password hash.
//!
//! The Postgres passwords cannot share that transaction. Each new password is
//! written to a pending file first, then `ALTER ROLE`, then the pending file
//! is promoted. A crash in either gap is finished on the next launch by trying
//! the current password and then the pending one.
//!
//! Desktop installs keep two passwords. `pg_password` is the bootstrap
//! superuser. The runtime role password is `pg_app_password`. Replace keys
//! rotates both. A non-desktop start still finishes a pending database
//! password, master key, or session secret left beside `config.toml`.

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
const PG_APP_FILE: &str = "pg_app_password";
const MASTER_PENDING: &str = "master_key.pending";
const SESSION_PENDING: &str = "session_secret.pending";
const ADMIN_PENDING: &str = "admin_password.pending";
pub const PG_PENDING: &str = "pg_password.pending";
const PG_APP_PENDING: &str = "pg_app_password.pending";

pub const REPLACE_UI_ERROR: &str = "Couldn't replace keys. Nothing was changed.";
pub const PASSWORD_EMPTY: &str = "Enter a new admin password.";
/// Shown when startup cannot replace a lost application password. The secret
/// itself is never included.
pub const HEAL_APP_PASSWORD_ERROR: &str =
    "Couldn't set a new database password. The password file was missing or unreadable.";
const HEAL_APP_PASSWORD_LOG: &str =
    "application database password file was missing or unreadable; a new password was set";
const SUPERUSER_RESTORE_LOG: &str =
    "new database superuser password could not be restored; the pending file was kept";
const SUPERUSER_DATABASE: &str = "postgres";
/// Bootstrap role created by `initdb -U coppice`.
pub const DESKTOP_SUPERUSER: &str = "coppice";

/// Where to set a lost `pg_app_password`, and the password to set.
pub struct AppPasswordHeal<'a> {
    pub host: &'a str,
    pub port: u16,
    pub superuser: &'a str,
    pub superuser_password: &'a str,
    pub role: &'a str,
    pub database: &'a str,
    pub password: &'a str,
}

#[derive(Debug, thiserror::Error)]
pub enum ReplaceError {
    #[error("{REPLACE_UI_ERROR}")]
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
    finish_role_file(dir, host, port, user, database, PG_FILE, PG_PENDING).await
}

/// Desktop runtime role. Same crash recovery as [`finish_file_password`], using
/// `pg_app_password` so the application password and the superuser password
/// stay in separate files.
pub async fn finish_app_password(
    dir: &Path,
    host: &str,
    port: u16,
    user: &str,
    database: &str,
) -> Result<String, ReplaceError> {
    finish_role_file(dir, host, port, user, database, PG_APP_FILE, PG_APP_PENDING).await
}

/// Sets `heal.password` on `heal.role` when that role already exists, then
/// writes `pg_app_password` mode `0600`. A missing role is left for
/// provisioning, which creates it with this password.
///
/// The password is not logged. The superuser connection uses the `postgres`
/// database so a `search_path` on the application database cannot redirect it.
pub async fn heal_app_password(dir: &Path, heal: AppPasswordHeal<'_>) -> Result<(), ReplaceError> {
    if heal.password.is_empty() {
        return Err(ReplaceError::Failed(HEAL_APP_PASSWORD_ERROR.into()));
    }
    let mut conn = connect_role(
        heal.host,
        heal.port,
        heal.superuser,
        heal.superuser_password,
        SUPERUSER_DATABASE,
    )
    .await
    .map_err(|_| ReplaceError::Failed(HEAL_APP_PASSWORD_ERROR.into()))?;
    let exists = role_exists(&mut conn, heal.role).await?;
    if exists {
        alter_role_password(&mut conn, heal.role, heal.password)
            .await
            .map_err(|_| ReplaceError::Failed(HEAL_APP_PASSWORD_ERROR.into()))?;
        let check_database = database_exists(&mut conn, heal.database).await?;
        drop(conn);
        if check_database
            && !password_connects(
                heal.host,
                heal.port,
                heal.role,
                heal.password,
                heal.database,
            )
            .await
        {
            return Err(ReplaceError::Failed(HEAL_APP_PASSWORD_ERROR.into()));
        }
    } else {
        drop(conn);
    }

    write_secret_atomic(&dir.join(PG_APP_FILE), heal.password)?;
    let _ = std::fs::remove_file(dir.join(PG_APP_PENDING));
    if exists {
        tracing::info!("{HEAL_APP_PASSWORD_LOG}");
    }
    Ok(())
}

/// Writes `pg_password.pending`, changes the superuser password, and leaves
/// the pending file in place until [`promote_pending`]. A failed attempt
/// removes the pending file and leaves the role password as it was.
async fn rotate_superuser_password(
    dir: &Path,
    host: &str,
    port: u16,
    superuser: &str,
    new_password: &str,
) -> Result<(), ReplaceError> {
    let pending_path = dir.join(PG_PENDING);
    let current = read_secret(&dir.join(PG_FILE))?;
    write_secret_atomic(&pending_path, new_password)?;
    let mut conn = match connect_role(host, port, superuser, &current, SUPERUSER_DATABASE).await {
        Ok(conn) => conn,
        Err(_) => {
            let _ = std::fs::remove_file(&pending_path);
            return Err(ReplaceError::Failed(
                "could not change the database superuser password".into(),
            ));
        }
    };
    if alter_role_password(&mut conn, superuser, new_password)
        .await
        .is_err()
    {
        drop(conn);
        let _ = std::fs::remove_file(&pending_path);
        return Err(ReplaceError::Failed(
            "could not change the database superuser password".into(),
        ));
    }
    drop(conn);
    if !password_connects(host, port, superuser, new_password, SUPERUSER_DATABASE).await {
        restore_superuser_password(host, port, superuser, &current, new_password, &pending_path)
            .await;
        return Err(ReplaceError::Failed(
            "could not change the database superuser password".into(),
        ));
    }
    Ok(())
}

async fn restore_superuser_password(
    host: &str,
    port: u16,
    superuser: &str,
    old_password: &str,
    new_password: &str,
    pending_path: &Path,
) {
    let mut conn = match connect_role(host, port, superuser, new_password, SUPERUSER_DATABASE).await
    {
        Ok(conn) => conn,
        Err(_) => {
            match connect_role(host, port, superuser, old_password, SUPERUSER_DATABASE).await {
                Ok(conn) => conn,
                Err(_) => {
                    tracing::error!("{SUPERUSER_RESTORE_LOG}");
                    return;
                }
            }
        }
    };
    if alter_role_password(&mut conn, superuser, old_password)
        .await
        .is_err()
    {
        tracing::error!("{SUPERUSER_RESTORE_LOG}");
        return;
    }
    let _ = std::fs::remove_file(pending_path);
}

async fn connect_role(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    database: &str,
) -> Result<sqlx::PgConnection, ReplaceError> {
    let opts = PgConnectOptions::new()
        .host(host)
        .port(port)
        .username(user)
        .password(password)
        .database(database)
        .options([("search_path", "pg_catalog")])
        .disable_statement_logging();
    match tokio::time::timeout(Duration::from_secs(4), opts.connect()).await {
        Ok(Ok(conn)) => Ok(conn),
        _ => Err(ReplaceError::Failed(
            "could not connect to the database".into(),
        )),
    }
}

async fn role_exists(conn: &mut sqlx::PgConnection, role: &str) -> Result<bool, ReplaceError> {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pg_catalog.pg_roles
            WHERE rolname::pg_catalog.text OPERATOR(pg_catalog.=) $1
        )",
    )
    .bind(role)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| ReplaceError::Failed(HEAL_APP_PASSWORD_ERROR.into()))
}

async fn database_exists(
    conn: &mut sqlx::PgConnection,
    database: &str,
) -> Result<bool, ReplaceError> {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pg_catalog.pg_database
            WHERE datname::pg_catalog.text OPERATOR(pg_catalog.=) $1
        )",
    )
    .bind(database)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| ReplaceError::Failed(HEAL_APP_PASSWORD_ERROR.into()))
}

async fn finish_role_file(
    dir: &Path,
    host: &str,
    port: u16,
    user: &str,
    database: &str,
    current_name: &str,
    pending_name: &str,
) -> Result<String, ReplaceError> {
    let current = read_secret(&dir.join(current_name))?;
    let pending_path = dir.join(pending_name);
    if !pending_path.is_file() {
        return Ok(current);
    }
    if password_connects(host, port, user, &current, database).await {
        let _ = std::fs::remove_file(&pending_path);
        return Ok(current);
    }
    let pending = read_secret(&pending_path)?;
    if password_connects(host, port, user, &pending, database).await {
        std::fs::rename(&pending_path, dir.join(current_name))?;
        return Ok(pending);
    }
    Err(ReplaceError::Failed(
        "the current postgres password and the pending replacement both failed to connect".into(),
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

/// Master key and session secret after a server crash.
///
/// The admin password is not recovered into config. The hash on the user row
/// is the record of it.
pub struct RecoveredServerKeys {
    pub master_key: String,
    pub session_secret: String,
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

/// Desktop Replace keys. Returns the database URL with the new application
/// role password. The pool's future connections use that password before this
/// returns. `superuser` is the bootstrap role (`coppice`); its password is
/// rotated through `pg_password.pending` as well.
pub async fn replace_desktop_keys(
    pool: &sqlx::PgPool,
    store: &SecretStore,
    dir: &Path,
    database_url: &str,
    admin_password: &str,
    admin_email: &str,
    superuser: &str,
) -> Result<String, ReplaceError> {
    let _guard = crate::crypto::operations_lock().lock().await;
    if !dir.join(MASTER_FILE).is_file() || !dir.join(SESSION_FILE).is_file() {
        return Err(ReplaceError::NotDesktop);
    }
    check_admin_password(admin_password)?;
    ensure_admin(pool, admin_email).await?;
    let database_url = settle_desktop_pg(pool, dir, database_url).await?;
    {
        let target = db_target(&database_url)?;
        finish_file_password(
            dir,
            &target.host,
            target.port,
            superuser,
            SUPERUSER_DATABASE,
        )
        .await?;
    }
    if let Some(key) = recover_pending_locked(pool, dir).await? {
        store.replace_master_key(&key);
    }

    let old_master = read_secret(&dir.join(MASTER_FILE))?;
    let _old_session = read_secret(&dir.join(SESSION_FILE))?;
    let new_master = generate_secret();
    let new_session = generate_secret();
    let new_pg = generate_secret();
    let new_super = generate_secret();
    let new_url = replace_url_password(&database_url, &new_pg)?;
    let password_hash = hash_password(admin_password)
        .map_err(|err| ReplaceError::Failed(format!("could not hash the admin password: {err}")))?;

    write_secret_atomic(&dir.join(PG_APP_PENDING), &new_pg)?;
    let old_options = (*pool.connect_options()).clone();
    let outcome = apply_pg_then_keys(
        pool,
        dir,
        &old_options,
        &database_url,
        &new_pg,
        &new_url,
        superuser,
        &new_super,
        &old_master,
        &new_master,
        &new_session,
        admin_password,
        admin_email,
        &password_hash,
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

#[allow(clippy::too_many_arguments)]
async fn apply_pg_then_keys(
    pool: &sqlx::PgPool,
    dir: &Path,
    old_options: &PgConnectOptions,
    database_url: &str,
    new_pg: &str,
    _new_url: &str,
    superuser: &str,
    new_super: &str,
    old_master: &str,
    new_master: &str,
    new_session: &str,
    admin_password: &str,
    admin_email: &str,
    password_hash: &str,
) -> Result<(), ReplaceError> {
    let target = db_target(database_url)?;
    let pg_pending = PG_APP_PENDING;
    let old_super = read_secret(&dir.join(PG_FILE))?;
    if let Err(err) =
        rotate_superuser_password(dir, &target.host, target.port, superuser, new_super).await
    {
        let _ = std::fs::remove_file(dir.join(pg_pending));
        return Err(err);
    }
    let mut conn = pool.acquire().await?;
    if let Err(err) = alter_role_password(&mut conn, &target.user, new_pg).await {
        let _ = std::fs::remove_file(dir.join(pg_pending));
        restore_superuser_password(
            &target.host,
            target.port,
            superuser,
            &old_super,
            new_super,
            &dir.join(PG_PENDING),
        )
        .await;
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
        if let Err(revert) = alter_role_password(&mut conn, &target.user, &target.password).await {
            tracing::error!(
                error = %revert,
                "new postgres password could not be restored; the pending file was kept"
            );
        } else {
            pool.set_connect_options(old_options.clone());
            let _ = std::fs::remove_file(dir.join(pg_pending));
        }
        restore_superuser_password(
            &target.host,
            target.port,
            superuser,
            &old_super,
            new_super,
            &dir.join(PG_PENDING),
        )
        .await;
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
            let _ = std::fs::remove_file(dir.join(pg_pending));
        }
        restore_superuser_password(
            &target.host,
            target.port,
            superuser,
            &old_super,
            new_super,
            &dir.join(PG_PENDING),
        )
        .await;
        return Err(err);
    }
    drop(conn);

    if let Err(err) = promote_pending(dir) {
        tracing::error!(
            error = %err,
            "keys were replaced in the database but the files were not moved; the next launch finishes that"
        );
        return Err(err);
    }
    Ok(())
}

async fn settle_desktop_pg(
    pool: &sqlx::PgPool,
    dir: &Path,
    database_url: &str,
) -> Result<String, ReplaceError> {
    if !dir.join(PG_APP_PENDING).is_file() {
        return Ok(database_url.to_string());
    }
    let target = db_target(database_url)?;
    let pg = finish_app_password(
        dir,
        &target.host,
        target.port,
        &target.user,
        &target.database,
    )
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
    let found: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM users WHERE email = $1 AND role = 'admin' LIMIT 1")
            .bind(email)
            .fetch_optional(pool)
            .await?;
    if found.is_none() {
        return Err(ReplaceError::Failed(format!("no admin user named {email}")));
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
    if role.is_empty() || !role.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(ReplaceError::Failed(format!(
            "unsupported database role {role}"
        )));
    }
    Ok(format!(
        "ALTER ROLE {role} PASSWORD {}",
        role_password_literal(password)?
    ))
}

/// Quoted SCRAM-SHA-256 verifier for `CREATE ROLE` / `ALTER ROLE`. The
/// plaintext password is not sent to PostgreSQL.
pub(crate) fn role_password_literal(password: &str) -> Result<String, ReplaceError> {
    if password.contains('\0') {
        return Err(ReplaceError::Failed("password contains a null byte".into()));
    }
    let verifier = scram_sha256_verifier(password)?;
    Ok(format!("'{}'", verifier.replace('\'', "''")))
}

/// PostgreSQL stores a client-built SCRAM-SHA-256 verifier as-is, so the
/// plaintext password is not sent in `ALTER ROLE` and cannot land in the
/// server log. The iteration count is the PostgreSQL 16 default.
fn scram_sha256_verifier(password: &str) -> Result<String, ReplaceError> {
    use base64::Engine;
    use hmac::Hmac;
    use sha2::{Digest, Sha256};

    let normalized = stringprep::saslprep(password)
        .map(|value| value.into_owned())
        .unwrap_or_else(|_| password.to_string());
    let mut salt = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    const ITERATIONS: u32 = 4096;
    let mut salted = [0u8; 32];
    pbkdf2::pbkdf2::<Hmac<Sha256>>(normalized.as_bytes(), &salt, ITERATIONS, &mut salted).map_err(
        |_| ReplaceError::Failed("could not build the database password verifier".into()),
    )?;
    let client_key = hmac_sha256(&salted, b"Client Key");
    let stored_key = Sha256::digest(client_key);
    let server_key = hmac_sha256(&salted, b"Server Key");
    let salt_b64 = base64::engine::general_purpose::STANDARD.encode(salt);
    let stored_b64 = base64::engine::general_purpose::STANDARD.encode(stored_key);
    let server_b64 = base64::engine::general_purpose::STANDARD.encode(server_key);
    Ok(format!(
        "SCRAM-SHA-256${ITERATIONS}:{salt_b64}${stored_b64}:{server_b64}"
    ))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("hmac-sha256 accepts a 32-byte key");
    mac.update(data);
    let bytes = mac.finalize().into_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
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
                settle_admin_file(pool, dir).await?;
                return Ok(None);
            }
            PendingKey::Committed => {
                promote_pending_except_admin(dir)?;
                settle_admin_file(pool, dir).await?;
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
    url: Option<&str>,
) -> Result<(), ReplaceError> {
    if master.is_some() || session.is_some() || url.is_some() {
        write_server_key_config(config_path, master, session, url)?;
    }
    for name in [MASTER_PENDING, SESSION_PENDING, ADMIN_PENDING] {
        let _ = std::fs::remove_file(dir.join(name));
    }
    Ok(())
}

/// Write rotated server secrets into `config.toml` and drop any admin password
/// stored there. The password hash on the user row is the only copy.
fn write_server_key_config(
    path: &Path,
    master: Option<&str>,
    session: Option<&str>,
    url: Option<&str>,
) -> Result<(), ReplaceError> {
    let mut pairs: Vec<(&[&str], &str)> = Vec::new();
    if let Some(master) = master {
        pairs.push((&["secrets", "master_key"], master));
    }
    if let Some(session) = session {
        pairs.push((&["auth", "session_secret"], session));
    }
    if let Some(url) = url {
        pairs.push((&["database", "url"], url));
    }
    if pairs.is_empty() {
        return Ok(());
    }
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
    for (keys, value) in pairs {
        set_nested(doc.as_table_mut(), keys, value)?;
    }
    remove_nested(doc.as_table_mut(), &["auth", "bootstrap_admin_password"]);
    coppice_config::connector_file::write_toml_atomic(path, &doc.to_string())
        .map_err(|err| ReplaceError::Failed(err.to_string()))?;
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
                let _ = std::fs::remove_file(dir.join(ADMIN_PENDING));
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

    if write_master || write_session {
        promote_server_config(
            config_path,
            dir,
            write_master.then_some(master.as_str()),
            write_session.then_some(session.as_str()),
            None,
        )?;
    }
    Ok(RecoveredServerKeys {
        master_key: master,
        session_secret: session,
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
    let row = sqlx::query("SELECT ciphertext, nonce FROM secrets WHERE NOT needs_reentry LIMIT 1")
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        // No token to decrypt, so the key files cannot show whether the
        // transaction committed. The admin password is settled against the
        // saved hash instead of being promoted with the key.
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
    let rows = sqlx::query("SELECT id, ciphertext, nonce, needs_reentry FROM secrets FOR UPDATE")
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
    let updated =
        sqlx::query("UPDATE users SET password_hash = $1 WHERE email = $2 AND role = 'admin'")
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
    promote_named(
        dir,
        &[
            (MASTER_PENDING, MASTER_FILE),
            (SESSION_PENDING, SESSION_FILE),
            (ADMIN_PENDING, ADMIN_FILE),
            (PG_PENDING, PG_FILE),
            (PG_APP_PENDING, PG_APP_FILE),
        ],
    )
}

/// Key and database files only. The admin password goes through
/// [`settle_admin_file`], which checks the saved hash first.
fn promote_pending_except_admin(dir: &Path) -> Result<(), ReplaceError> {
    promote_named(
        dir,
        &[
            (MASTER_PENDING, MASTER_FILE),
            (SESSION_PENDING, SESSION_FILE),
            (PG_PENDING, PG_FILE),
            (PG_APP_PENDING, PG_APP_FILE),
        ],
    )
}

fn promote_named(dir: &Path, pairs: &[(&str, &str)]) -> Result<(), ReplaceError> {
    for (pending, current) in pairs {
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
    let password = password_from_url(url)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ReplaceError::Failed("database url has no password".into()))?;
    Ok(DbTarget {
        host: opts.get_host().to_string(),
        port: opts.get_port(),
        user: opts.get_username().to_string(),
        password,
        database: opts.get_database().unwrap_or("postgres").to_string(),
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
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| ReplaceError::Failed("database url has no scheme".into()))?;
    let auth_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..auth_end];
    let tail = &rest[auth_end..];
    let (userinfo, host) = authority
        .rsplit_once('@')
        .ok_or_else(|| ReplaceError::Failed("database url has no user".into()))?;
    let user = userinfo
        .split_once(':')
        .map(|(user, _)| user)
        .unwrap_or(userinfo);
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

fn remove_nested(table: &mut toml_edit::Table, keys: &[&str]) {
    let Some((first, rest)) = keys.split_first() else {
        return;
    };
    if rest.is_empty() {
        table.remove(first);
        return;
    }
    let remove_parent = {
        let Some(child) = table.get_mut(first).and_then(|item| item.as_table_mut()) else {
            return;
        };
        remove_nested(child, rest);
        child.is_empty()
    };
    if remove_parent {
        table.remove(first);
    }
}

fn set_nested(
    table: &mut toml_edit::Table,
    keys: &[&str],
    value: &str,
) -> Result<(), ReplaceError> {
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
        .database(database)
        .options([("search_path", "pg_catalog")]);
    match tokio::time::timeout(Duration::from_secs(4), opts.connect()).await {
        Ok(Ok(conn)) => {
            let _ = conn.close().await;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod plain_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn server_key_config_omits_admin_password_and_is_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[auth]\nbootstrap_admin_password = \"admin-plain-password\"\nsession_secret = \"old\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        write_server_key_config(
            &path,
            Some("master-secret-value"),
            Some("session-secret-value"),
            Some("postgres://coppice:pg-secret-value@127.0.0.1:5432/coppice"),
        )
        .unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("master-secret-value"), "{text}");
        assert!(text.contains("session-secret-value"), "{text}");
        assert!(text.contains("pg-secret-value"), "{text}");
        assert!(!text.contains("admin-plain-password"), "{text}");
        assert!(!text.contains("bootstrap_admin_password"), "{text}");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn heal_and_restore_messages_do_not_include_a_secret() {
        for text in [
            HEAL_APP_PASSWORD_ERROR,
            HEAL_APP_PASSWORD_LOG,
            SUPERUSER_RESTORE_LOG,
        ] {
            let lower = text.to_ascii_lowercase();
            assert!(!lower.contains("mock"), "{text}");
            assert!(!lower.contains("fake"), "{text}");
            assert!(!text.contains("password="), "{text}");
        }
        assert_eq!(
            HEAL_APP_PASSWORD_ERROR,
            "Couldn't set a new database password. The password file was missing or unreadable."
        );
    }

    #[test]
    fn alter_role_sql_sends_a_scram_verifier_not_the_password() {
        let password = "s3cret-pass'word";
        let sql = alter_role_sql("coppice", password).unwrap();
        assert!(!sql.contains(password), "{sql}");
        assert!(!sql.contains("s3cret-pass"), "{sql}");
        assert!(
            sql.starts_with("ALTER ROLE coppice PASSWORD 'SCRAM-SHA-256$"),
            "{sql}"
        );
        assert!(sql.ends_with('\''), "{sql}");
        assert!(!sql.contains("PASSWORD 's"), "{sql}");
    }
}

#[cfg(all(test, feature = "embedded-test-db"))]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::str::FromStr;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::crypto::SecretStore;
    use sqlx::Row;
    use tracing_subscriber::layer::SubscriberExt;
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
        sqlx::query("INSERT INTO secrets (id, name, ciphertext, nonce) VALUES ($1, $2, $3, $4)")
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

        let still: i64 = sqlx::query_scalar("SELECT count(*) FROM secrets WHERE needs_reentry")
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

        let resolved =
            finish_file_password(&dir, &role.host, role.port, &role.name, &role.database)
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

        let resolved =
            finish_file_password(&dir, &role.host, role.port, &role.name, &role.database)
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

    #[tokio::test]
    async fn crash_before_commit_with_no_tokens_keeps_the_current_admin_password() {
        let pool = pool().await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        let old_master = generate_secret();
        write_pair(&dir, &old_master, &generate_secret());
        write_password_atomic(&dir.join(ADMIN_FILE), "old-admin-pass").unwrap();
        insert_admin(&pool, "admin@localhost", "old-admin-pass").await;
        write_secret_atomic(&dir.join(MASTER_PENDING), &generate_secret()).unwrap();
        write_secret_atomic(&dir.join(SESSION_PENDING), &generate_secret()).unwrap();
        write_password_atomic(&dir.join(ADMIN_PENDING), "new-admin-pass").unwrap();

        let promoted = recover_pending(&pool, &dir).await.expect("recover");
        assert!(promoted.is_some());
        assert_eq!(
            read_password_file(&dir.join(ADMIN_FILE)).unwrap(),
            "old-admin-pass"
        );
        assert!(!dir.join(ADMIN_PENDING).exists());
    }

    #[tokio::test]
    async fn committed_admin_password_is_kept_when_there_are_no_tokens() {
        let pool = pool().await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        let new_master = generate_secret();
        write_pair(&dir, &generate_secret(), &generate_secret());
        write_password_atomic(&dir.join(ADMIN_FILE), "old-admin-pass").unwrap();
        insert_admin(&pool, "admin@localhost", "new-admin-pass").await;
        write_secret_atomic(&dir.join(MASTER_PENDING), &new_master).unwrap();
        write_password_atomic(&dir.join(ADMIN_PENDING), "new-admin-pass").unwrap();

        let promoted = recover_pending(&pool, &dir).await.expect("recover");
        assert_eq!(promoted.as_deref(), Some(new_master.as_str()));
        assert_eq!(
            read_password_file(&dir.join(ADMIN_FILE)).unwrap(),
            "new-admin-pass"
        );
        assert!(!dir.join(ADMIN_PENDING).exists());
    }

    #[tokio::test]
    async fn alter_role_uses_scram_verifier_and_plaintext_still_connects() {
        let pool = pool().await;
        let password = "quote'pass-99";
        let role = RoleGuard::create(&pool, "temp-init-pass").await;
        let sql = alter_role_sql(&role.name, password).unwrap();
        assert!(!sql.contains(password), "{sql}");
        assert!(sql.contains("SCRAM-SHA-256$"), "{sql}");
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query(&sql).execute(&mut *conn).await.unwrap();
        drop(conn);

        assert!(
            password_connects(&role.host, role.port, &role.name, password, &role.database).await
        );
        assert!(
            !password_connects(
                &role.host,
                role.port,
                &role.name,
                "temp-init-pass",
                &role.database
            )
            .await
        );
        role.drop_role().await;
    }

    #[test]
    fn promote_pending_moves_the_app_password_and_leaves_the_superuser_password() {
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        let superuser = generate_secret();
        let old_app = generate_secret();
        let new_app = generate_secret();
        write_secret_atomic(&dir.join(PG_FILE), &superuser).unwrap();
        write_secret_atomic(&dir.join(PG_APP_FILE), &old_app).unwrap();
        write_secret_atomic(&dir.join(PG_APP_PENDING), &new_app).unwrap();

        promote_pending(&dir).unwrap();

        assert_eq!(read_secret(&dir.join(PG_FILE)).unwrap(), superuser);
        assert_eq!(read_secret(&dir.join(PG_APP_FILE)).unwrap(), new_app);
        assert!(!dir.join(PG_APP_PENDING).exists());
    }

    struct SuperLogin {
        host: String,
        port: u16,
        user: String,
        password: String,
    }

    async fn super_login() -> SuperLogin {
        let url = crate::db::session_database_url()
            .await
            .expect("session url");
        let opts = PgConnectOptions::from_str(&url).expect("session url");
        SuperLogin {
            host: opts.get_host().to_string(),
            port: opts.get_port(),
            user: opts.get_username().to_string(),
            password: password_from_url(&url).expect("password"),
        }
    }

    async fn grant_postgres_connect(pool: &sqlx::PgPool, role: &str) {
        sqlx::query(&format!("GRANT CONNECT ON DATABASE postgres TO {role}"))
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(&format!("ALTER ROLE {role} SUPERUSER"))
            .execute(pool)
            .await
            .unwrap();
    }

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[derive(Clone, Default)]
    struct LogBag(Arc<Mutex<String>>);

    struct LogLayer(LogBag);

    struct LogVisit(String);

    impl tracing::field::Visit for LogVisit {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.0.push_str(field.name());
            self.0.push('=');
            self.0.push_str(value);
            self.0.push('\n');
        }

        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0.push_str(field.name());
            self.0.push('=');
            self.0.push_str(&format!("{value:?}"));
            self.0.push('\n');
        }
    }

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LogLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut visit = LogVisit(String::new());
            event.record(&mut visit);
            let mut buf = self.0 .0.lock().expect("logs");
            buf.push_str(event.metadata().target());
            buf.push(' ');
            buf.push_str(&visit.0);
        }
    }

    fn capture_logs() -> (LogBag, tracing::subscriber::DefaultGuard) {
        let bag = LogBag::default();
        let subscriber = tracing_subscriber::registry().with(LogLayer(bag.clone()));
        let guard = tracing::subscriber::set_default(subscriber);
        (bag, guard)
    }

    fn assert_logs_omit(logs: &LogBag, secrets: &[&str]) {
        let text = logs.0.lock().expect("logs").clone();
        for secret in secrets {
            assert!(!secret.is_empty());
            assert!(
                !text.contains(secret),
                "a secret was written to the log: {text}"
            );
        }
    }

    fn heal_for<'a>(
        login: &'a SuperLogin,
        role: &'a str,
        database: &'a str,
        password: &'a str,
    ) -> AppPasswordHeal<'a> {
        AppPasswordHeal {
            host: &login.host,
            port: login.port,
            superuser: &login.user,
            superuser_password: &login.password,
            role,
            database,
            password,
        }
    }

    #[tokio::test]
    async fn lost_app_password_is_reset_with_the_superuser_connection() {
        let pool = pool().await;
        let login = super_login().await;
        let old_password = generate_secret();
        let new_password = generate_secret();
        let stale_pending = generate_secret();
        let role = RoleGuard::create(&pool, &old_password).await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        write_secret_atomic(&dir.join(PG_APP_PENDING), &stale_pending).unwrap();
        let (logs, _guard) = capture_logs();

        heal_app_password(
            &dir,
            heal_for(&login, &role.name, &role.database, &new_password),
        )
        .await
        .expect("heal");

        assert_eq!(read_secret(&dir.join(PG_APP_FILE)).unwrap(), new_password);
        assert_eq!(mode_of(&dir.join(PG_APP_FILE)), 0o600);
        assert!(!dir.join(PG_APP_PENDING).exists());
        assert!(
            password_connects(
                &role.host,
                role.port,
                &role.name,
                &new_password,
                &role.database
            )
            .await
        );
        assert!(
            !password_connects(
                &role.host,
                role.port,
                &role.name,
                &old_password,
                &role.database
            )
            .await
        );
        assert!(
            !password_connects(
                &role.host,
                role.port,
                &role.name,
                &stale_pending,
                &role.database
            )
            .await
        );
        let text = logs.0.lock().expect("logs").clone();
        assert!(text.contains(HEAL_APP_PASSWORD_LOG), "{text}");
        assert_logs_omit(&logs, &[&old_password, &new_password, &stale_pending]);
        role.drop_role().await;
    }

    #[tokio::test]
    async fn unreadable_app_password_file_is_replaced_and_not_logged() {
        let pool = pool().await;
        let login = super_login().await;
        let old_password = generate_secret();
        let new_password = generate_secret();
        let role = RoleGuard::create(&pool, &old_password).await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        let path = dir.join(PG_APP_FILE);
        std::fs::write(&path, "not-a-password\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
        let (logs, _guard) = capture_logs();

        heal_app_password(
            &dir,
            heal_for(&login, &role.name, &role.database, &new_password),
        )
        .await
        .expect("heal");

        assert_eq!(read_secret(&path).unwrap(), new_password);
        assert_eq!(mode_of(&path), 0o600);
        assert!(
            password_connects(
                &role.host,
                role.port,
                &role.name,
                &new_password,
                &role.database
            )
            .await
        );
        assert!(
            !password_connects(
                &role.host,
                role.port,
                &role.name,
                &old_password,
                &role.database
            )
            .await
        );
        assert_logs_omit(&logs, &[&old_password, &new_password, "not-a-password"]);
        role.drop_role().await;
    }

    #[tokio::test]
    async fn missing_app_password_is_written_when_the_role_does_not_exist_yet() {
        let _pool = pool().await;
        let login = super_login().await;
        let new_password = generate_secret();
        let role = format!("absent_{:x}", Uuid::new_v4().as_u128());
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        let (logs, _guard) = capture_logs();

        heal_app_password(&dir, heal_for(&login, &role, "postgres", &new_password))
            .await
            .expect("write");

        assert_eq!(read_secret(&dir.join(PG_APP_FILE)).unwrap(), new_password);
        assert_eq!(mode_of(&dir.join(PG_APP_FILE)), 0o600);
        let text = logs.0.lock().expect("logs").clone();
        assert!(!text.contains(HEAL_APP_PASSWORD_LOG), "{text}");
        assert_logs_omit(&logs, &[&new_password]);
    }

    #[tokio::test]
    async fn app_password_reset_fails_when_the_superuser_password_is_wrong() {
        let pool = pool().await;
        let login = super_login().await;
        let old_password = generate_secret();
        let new_password = generate_secret();
        let role = RoleGuard::create(&pool, &old_password).await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        let mut wrong = login;
        wrong.password = generate_secret();
        let (logs, _guard) = capture_logs();

        let err = heal_app_password(
            &dir,
            heal_for(&wrong, &role.name, &role.database, &new_password),
        )
        .await
        .expect_err("superuser password");

        assert_eq!(err.to_string(), HEAL_APP_PASSWORD_ERROR);
        assert!(!dir.join(PG_APP_FILE).exists());
        assert!(
            password_connects(
                &role.host,
                role.port,
                &role.name,
                &old_password,
                &role.database
            )
            .await
        );
        assert!(
            !password_connects(
                &role.host,
                role.port,
                &role.name,
                &new_password,
                &role.database
            )
            .await
        );
        assert_logs_omit(
            &logs,
            &[&old_password, &new_password, wrong.password.as_str()],
        );
        role.drop_role().await;
    }

    #[tokio::test]
    async fn crash_after_superuser_alter_is_finished_on_the_next_launch() {
        let pool = pool().await;
        let old_password = generate_secret();
        let new_password = generate_secret();
        let role = RoleGuard::create(&pool, &old_password).await;
        grant_postgres_connect(&pool, &role.name).await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        write_secret_atomic(&dir.join(PG_FILE), &old_password).unwrap();

        rotate_superuser_password(&dir, &role.host, role.port, &role.name, &new_password)
            .await
            .expect("rotate");

        assert_eq!(read_secret(&dir.join(PG_FILE)).unwrap(), old_password);
        assert_eq!(read_secret(&dir.join(PG_PENDING)).unwrap(), new_password);
        assert_eq!(mode_of(&dir.join(PG_PENDING)), 0o600);
        assert!(
            password_connects(&role.host, role.port, &role.name, &new_password, "postgres").await
        );
        assert!(
            !password_connects(&role.host, role.port, &role.name, &old_password, "postgres").await
        );

        let resolved = finish_file_password(&dir, &role.host, role.port, &role.name, "postgres")
            .await
            .expect("finish");
        assert_eq!(resolved, new_password);
        assert_eq!(read_secret(&dir.join(PG_FILE)).unwrap(), new_password);
        assert!(!dir.join(PG_PENDING).exists());
        assert_eq!(mode_of(&dir.join(PG_FILE)), 0o600);
        role.drop_role().await;
    }

    #[tokio::test]
    async fn superuser_rotation_keeps_the_role_when_the_file_password_does_not_connect() {
        let pool = pool().await;
        let real_password = generate_secret();
        let file_password = generate_secret();
        let proposed = generate_secret();
        let role = RoleGuard::create(&pool, &real_password).await;
        grant_postgres_connect(&pool, &role.name).await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        write_secret_atomic(&dir.join(PG_FILE), &file_password).unwrap();

        let err = rotate_superuser_password(&dir, &role.host, role.port, &role.name, &proposed)
            .await
            .expect_err("connect");

        let text = err.to_string();
        for secret in [&real_password, &file_password, &proposed] {
            assert!(!text.contains(secret.as_str()), "{text}");
        }
        assert!(!dir.join(PG_PENDING).exists());
        assert_eq!(read_secret(&dir.join(PG_FILE)).unwrap(), file_password);
        assert!(
            password_connects(
                &role.host,
                role.port,
                &role.name,
                &real_password,
                "postgres"
            )
            .await
        );
        assert!(!password_connects(&role.host, role.port, &role.name, &proposed, "postgres").await);
        role.drop_role().await;
    }

    #[tokio::test]
    async fn replace_keys_rotates_the_superuser_password_and_the_app_password() {
        let pool = pool().await;
        let login = super_login().await;
        let old_app = generate_secret();
        let old_super = generate_secret();
        let app = RoleGuard::create(&pool, &old_app).await;
        let su = RoleGuard::create(&pool, &old_super).await;
        assert_ne!(su.name, login.user);
        grant_postgres_connect(&pool, &su.name).await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        let old_master = generate_secret();
        let old_session = generate_secret();
        write_pair(&dir, &old_master, &old_session);
        write_password_atomic(&dir.join(ADMIN_FILE), "old-admin-pass").unwrap();
        write_secret_atomic(&dir.join(PG_FILE), &old_super).unwrap();
        write_secret_atomic(&dir.join(PG_APP_FILE), &old_app).unwrap();
        let store = SecretStore::from_master_key(&old_master);
        insert_secret(&pool, &store, "forge-token-plain").await;
        insert_admin(&pool, "admin@localhost", "old-admin-pass").await;
        let app_url = format!(
            "postgres://{}:{}@{}:{}/{}",
            app.name, old_app, app.host, app.port, app.database
        );
        let (logs, _guard) = capture_logs();

        let new_url = replace_desktop_keys(
            &pool,
            &store,
            &dir,
            &app_url,
            "new-admin-pass",
            "admin@localhost",
            &su.name,
        )
        .await
        .expect("replace");

        let new_app = read_secret(&dir.join(PG_APP_FILE)).unwrap();
        let new_super = read_secret(&dir.join(PG_FILE)).unwrap();
        assert_ne!(new_app, old_app);
        assert_ne!(new_super, old_super);
        assert_ne!(new_app, new_super);
        assert!(new_url.contains(&new_app));
        assert!(!new_url.contains(&old_app));
        assert!(!dir.join(PG_PENDING).exists());
        assert!(!dir.join(PG_APP_PENDING).exists());
        assert_eq!(mode_of(&dir.join(PG_FILE)), 0o600);
        assert_eq!(mode_of(&dir.join(PG_APP_FILE)), 0o600);
        assert!(password_connects(&su.host, su.port, &su.name, &new_super, "postgres").await);
        assert!(!password_connects(&su.host, su.port, &su.name, &old_super, "postgres").await);
        assert!(password_connects(&app.host, app.port, &app.name, &new_app, &app.database).await);
        assert!(!password_connects(&app.host, app.port, &app.name, &old_app, &app.database).await);
        assert!(
            password_connects(
                &login.host,
                login.port,
                &login.user,
                &login.password,
                "postgres"
            )
            .await,
            "the embedded superuser password must stay in place"
        );
        assert_logs_omit(
            &logs,
            &[
                old_app.as_str(),
                old_super.as_str(),
                new_app.as_str(),
                new_super.as_str(),
                old_master.as_str(),
                old_session.as_str(),
            ],
        );
        app.drop_role().await;
        su.drop_role().await;
    }

    #[tokio::test]
    async fn failed_replace_restores_the_superuser_password() {
        let pool = pool().await;
        let login = super_login().await;
        let old_super = generate_secret();
        let su = RoleGuard::create(&pool, &old_super).await;
        grant_postgres_connect(&pool, &su.name).await;
        let root = secrets_dir();
        let dir = root.path().join("secrets");
        let old_master = generate_secret();
        write_pair(&dir, &old_master, &generate_secret());
        write_password_atomic(&dir.join(ADMIN_FILE), "old-admin-pass").unwrap();
        write_secret_atomic(&dir.join(PG_FILE), &old_super).unwrap();
        write_secret_atomic(&dir.join(PG_APP_FILE), &generate_secret()).unwrap();
        insert_admin(&pool, "admin@localhost", "old-admin-pass").await;
        let store = SecretStore::from_master_key(&old_master);
        let bad_url = format!(
            "postgres://bad-name:old-app-pass@{}:{}/postgres",
            login.host, login.port
        );

        let err = replace_desktop_keys(
            &pool,
            &store,
            &dir,
            &bad_url,
            "new-admin-pass",
            "admin@localhost",
            &su.name,
        )
        .await
        .expect_err("invalid role");

        let text = err.to_string();
        assert!(!text.contains(&old_super), "{text}");
        assert!(!dir.join(PG_PENDING).exists());
        assert!(!dir.join(PG_APP_PENDING).exists());
        assert_eq!(read_secret(&dir.join(PG_FILE)).unwrap(), old_super);
        assert!(password_connects(&su.host, su.port, &su.name, &old_super, "postgres").await);
        su.drop_role().await;
    }
}

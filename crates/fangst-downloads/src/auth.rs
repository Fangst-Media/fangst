//! Users, their API keys and sessions, and secrets kept for them, all in `downloads.db`.
//!
//! Passwords are stored as Argon2id hashes. API keys and secrets are encrypted under the master
//! key; API keys are found by a keyed hash. Session tokens are stored as SHA-256 hashes only.

use std::sync::Arc;
use std::time::Duration;

use fangst_core::secret::{self, MasterKey};
use sqlx::FromRow;
use sqlx::sqlite::SqlitePool;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::{Error, Store, now};

/// The user created on first start, who owns the operator's API key.
pub const FIRST_USER: &str = "admin";

/// Name of the API key set from the environment on every start.
pub const ENVIRONMENT_KEY: &str = "environment";

/// Twelve characters keeps online guessing hopeless under the login rate limit while staying
/// typeable; there is no upper bound on character classes, only length.
pub const MIN_PASSWORD_CHARS: usize = 12;

/// Argon2 cost does not grow with length, but the request body should stay small.
const MAX_PASSWORD_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct User {
    pub id: i64,
    pub username: String,
}

#[derive(Clone)]
pub struct Auth {
    pool: SqlitePool,
    key: Arc<MasterKey>,
    hasher: Arc<Hasher>,
}

impl Auth {
    /// # Errors
    /// When the random source fails while preparing the decoy hash.
    pub async fn new(store: &Store, key: Arc<MasterKey>) -> Result<Self, Error> {
        Self::with_hash_slots(store, key, default_hash_slots()).await
    }

    /// Like [`Auth::new`] with an explicit number of password hashes that may run at once.
    ///
    /// # Errors
    /// When the random source fails while preparing the decoy hash.
    pub async fn with_hash_slots(
        store: &Store,
        key: Arc<MasterKey>,
        slots: usize,
    ) -> Result<Self, Error> {
        Ok(Self {
            pool: store.pool.clone(),
            key,
            hasher: Arc::new(Hasher::new(slots).await?),
        })
    }

    /// Makes `api_key` the first user's `environment` key, creating that user on a fresh database.
    /// A key from an earlier start stops working, so rotating the variable revokes the old key,
    /// and the first user's sessions end with it: whoever held the old key may have opened them.
    ///
    /// # Errors
    /// When the database write fails.
    pub async fn bootstrap(&self, api_key: &str) -> Result<User, Error> {
        let first: Option<User> =
            sqlx::query_as("SELECT id, username FROM users ORDER BY id LIMIT 1")
                .fetch_optional(&self.pool)
                .await?;
        let user = match first {
            Some(user) => user,
            None => self.create_user(FIRST_USER, None).await?,
        };
        let previous: Option<String> =
            sqlx::query_scalar("SELECT lookup FROM api_keys WHERE user_id = ? AND name = ?")
                .bind(user.id)
                .bind(ENVIRONMENT_KEY)
                .fetch_optional(&self.pool)
                .await?;
        let changed = previous.is_some_and(|p| p != self.key.lookup_hash(api_key.as_bytes()));
        self.set_api_key(user.id, ENVIRONMENT_KEY, api_key).await?;
        if changed {
            self.end_sessions(user.id).await?;
        }
        Ok(user)
    }

    /// The first user, who owns the daemon and may change its settings. `None` before the first
    /// start created one.
    ///
    /// # Errors
    /// When the database read fails.
    pub async fn owner(&self) -> Result<Option<User>, Error> {
        Ok(
            sqlx::query_as("SELECT id, username FROM users ORDER BY id LIMIT 1")
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// # Errors
    /// When the database read fails.
    pub async fn is_owner(&self, user_id: i64) -> Result<bool, Error> {
        Ok(self.owner().await?.is_some_and(|owner| owner.id == user_id))
    }

    /// # Errors
    /// When the database read fails.
    pub async fn user_by_name(&self, username: &str) -> Result<Option<User>, Error> {
        Ok(
            sqlx::query_as("SELECT id, username FROM users WHERE username = ?")
                .bind(username.trim())
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// Signs the user out everywhere.
    ///
    /// # Errors
    /// When the database write fails.
    pub async fn end_sessions(&self, user_id: i64) -> Result<(), Error> {
        sqlx::query("DELETE FROM sessions WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// # Errors
    /// [`Error::Invalid`] for a taken name or a password that is too short.
    pub async fn create_user(&self, username: &str, password: Option<&str>) -> Result<User, Error> {
        let username = username.trim();
        if username.is_empty() {
            return Err(Error::Invalid("the username can't be empty".to_owned()));
        }
        let hash = match password {
            Some(password) => Some(self.hasher.hash(password).await?),
            None => None,
        };
        let id: Option<i64> = sqlx::query_scalar(
            "INSERT INTO users (username, password_hash, created_at) VALUES (?, ?, ?)
             ON CONFLICT (username) DO NOTHING RETURNING id",
        )
        .bind(username)
        .bind(hash)
        .bind(now())
        .fetch_optional(&self.pool)
        .await?;
        let id = id.ok_or_else(|| Error::Invalid(format!("the user {username} already exists")))?;
        Ok(User {
            id,
            username: username.to_owned(),
        })
    }

    /// # Errors
    /// [`Error::Invalid`] for a password that is too short or too long.
    pub async fn set_password(&self, user_id: i64, password: &str) -> Result<(), Error> {
        let hash = self.hasher.hash(password).await?;
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(hash)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        // Sessions opened with the old password end with it.
        sqlx::query("DELETE FROM sessions WHERE user_id = ?")
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// # Errors
    /// When the database read fails.
    pub async fn has_password(&self, user_id: i64) -> Result<bool, Error> {
        let hash: Option<Option<String>> =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(hash.flatten().is_some())
    }

    /// The user when name and password match. An unknown user, a user without a password and a
    /// wrong password all take one Argon2 verification, so timing doesn't tell them apart.
    ///
    /// # Errors
    /// When the database read fails.
    pub async fn login(&self, username: &str, password: &str) -> Result<Option<User>, Error> {
        let row: Option<(i64, String, Option<String>)> =
            sqlx::query_as("SELECT id, username, password_hash FROM users WHERE username = ?")
                .bind(username.trim())
                .fetch_optional(&self.pool)
                .await?;
        let (user, stored) = match row {
            Some((id, username, Some(hash))) => (Some(User { id, username }), Some(hash)),
            _ => (None, None),
        };
        let ok = self.hasher.verify(stored, password).await?;
        Ok(user.filter(|_| ok))
    }

    /// # Errors
    /// When the database read fails.
    pub async fn check_password(&self, user_id: i64, password: &str) -> Result<bool, Error> {
        let stored: Option<Option<String>> =
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(&self.pool)
                .await?;
        match stored.flatten() {
            Some(hash) => self.hasher.verify(Some(hash), password).await,
            None => Ok(false),
        }
    }

    /// Stores `api_key` for the user under `name`, replacing the key that name held before.
    ///
    /// # Errors
    /// [`Error::Invalid`] when another user already holds the same key.
    pub async fn set_api_key(&self, user_id: i64, name: &str, api_key: &str) -> Result<(), Error> {
        let lookup = self.key.lookup_hash(api_key.as_bytes());
        let sealed = self.key.seal(
            api_key_context(user_id, name).as_bytes(),
            api_key.as_bytes(),
        )?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM api_keys WHERE user_id = ? AND name = ?")
            .bind(user_id)
            .bind(name)
            .execute(&mut *tx)
            .await?;
        let inserted = sqlx::query(
            "INSERT INTO api_keys (user_id, name, lookup, sealed, created_at) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (lookup) DO NOTHING",
        )
        .bind(user_id)
        .bind(name)
        .bind(lookup)
        .bind(sealed)
        .bind(now())
        .execute(&mut *tx)
        .await?;
        if inserted.rows_affected() == 0 {
            return Err(Error::Invalid("that API key is already in use".to_owned()));
        }
        tx.commit().await?;
        Ok(())
    }

    /// A user's API key as it was given, for showing it to its owner.
    ///
    /// # Errors
    /// When the database read fails or the stored key can't be decrypted.
    pub async fn api_key(&self, user_id: i64, name: &str) -> Result<Option<String>, Error> {
        let sealed: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT sealed FROM api_keys WHERE user_id = ? AND name = ?")
                .bind(user_id)
                .bind(name)
                .fetch_optional(&self.pool)
                .await?;
        sealed
            .map(|sealed| {
                let plain = self
                    .key
                    .open(api_key_context(user_id, name).as_bytes(), &sealed)?;
                String::from_utf8(plain)
                    .map_err(|_| Error::Corrupt("an API key is not text".to_owned()))
            })
            .transpose()
    }

    /// # Errors
    /// When the database read fails.
    pub async fn user_for_api_key(&self, api_key: &str) -> Result<Option<User>, Error> {
        Ok(sqlx::query_as(
            "SELECT users.id, users.username FROM api_keys JOIN users ON users.id = api_keys.user_id
             WHERE api_keys.lookup = ?",
        )
        .bind(self.key.lookup_hash(api_key.as_bytes()))
        .fetch_optional(&self.pool)
        .await?)
    }

    /// Opens a session and returns its token. Expired sessions are cleared at the same time.
    ///
    /// # Errors
    /// When the random source or the database write fails.
    pub async fn start_session(&self, user_id: i64, lifetime: Duration) -> Result<String, Error> {
        let token = secret::random_token()?;
        let now = now();
        let expires = now.saturating_add(i64::try_from(lifetime.as_secs()).unwrap_or(i64::MAX));
        sqlx::query("DELETE FROM sessions WHERE expires_at <= ?")
            .bind(now)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(secret::sha256_hex(token.as_bytes()))
        .bind(user_id)
        .bind(now)
        .bind(expires)
        .execute(&self.pool)
        .await?;
        Ok(token)
    }

    /// # Errors
    /// When the database read fails.
    pub async fn user_for_session(&self, token: &str) -> Result<Option<User>, Error> {
        Ok(sqlx::query_as(
            "SELECT users.id, users.username FROM sessions JOIN users ON users.id = sessions.user_id
             WHERE sessions.token_hash = ? AND sessions.expires_at > ?",
        )
        .bind(secret::sha256_hex(token.as_bytes()))
        .bind(now())
        .fetch_optional(&self.pool)
        .await?)
    }

    /// # Errors
    /// When the database write fails.
    pub async fn end_session(&self, token: &str) -> Result<(), Error> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(secret::sha256_hex(token.as_bytes()))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn api_key_context(user_id: i64, name: &str) -> String {
    format!("api_key:{user_id}:{name}")
}

fn check_length(password: &str) -> Result<(), Error> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(Error::Invalid(format!(
            "the password must be at least {MIN_PASSWORD_CHARS} characters"
        )));
    }
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(Error::Invalid(format!(
            "the password must be at most {MAX_PASSWORD_BYTES} bytes"
        )));
    }
    Ok(())
}

/// Runs Argon2 off the async threads, a bounded number at a time.
///
/// Each run holds 19 MiB and tens of milliseconds of CPU on purpose. Unbounded, a burst of logins
/// would exhaust memory; queued, it would pile up waiting requests. Past the limit callers get
/// [`Error::Busy`] right away and can retry.
struct Hasher {
    slots: Arc<Semaphore>,
    /// A real hash of a random value nobody knows, made at start so the first unknown-user login
    /// costs the same as every later one.
    decoy: String,
}

impl Hasher {
    async fn new(slots: usize) -> Result<Self, Error> {
        let decoy = tokio::task::spawn_blocking(|| secret::hash_password(&secret::random_token()?))
            .await
            .map_err(|e| Error::Corrupt(e.to_string()))??;
        Ok(Self {
            slots: Arc::new(Semaphore::new(slots.max(1))),
            decoy,
        })
    }

    fn slot(&self) -> Result<OwnedSemaphorePermit, Error> {
        Arc::clone(&self.slots)
            .try_acquire_owned()
            .map_err(|_| Error::Busy)
    }

    async fn hash(&self, password: &str) -> Result<String, Error> {
        check_length(password)?;
        let permit = self.slot()?;
        let password = password.to_owned();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            secret::hash_password(&password)
        })
        .await
        .map_err(|e| Error::Corrupt(e.to_string()))?
        .map_err(Error::Secret)
    }

    /// `None` verifies against the decoy, so a missing user costs the same as a wrong password.
    async fn verify(&self, hash: Option<String>, password: &str) -> Result<bool, Error> {
        if password.len() > MAX_PASSWORD_BYTES {
            return Ok(false);
        }
        let permit = self.slot()?;
        let password = password.to_owned();
        let decoy = hash.is_none().then(|| self.decoy.clone());
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            match (hash, decoy) {
                (Some(hash), _) => secret::verify_password(&hash, &password),
                (None, Some(decoy)) => {
                    let _ = secret::verify_password(&decoy, &password);
                    false
                }
                (None, None) => false,
            }
        })
        .await
        .map_err(|e| Error::Corrupt(e.to_string()))
    }
}

/// One Argon2 run per core, but never more than this many at once: four runs hold 76 MiB, which a
/// small home server can spare, and logins are rare enough that more parallelism buys nothing.
const MAX_HASH_SLOTS: usize = 4;

fn default_hash_slots() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(MAX_HASH_SLOTS)
}

/// Credentials the daemon needs in plain text later, such as usenet server passwords, kept
/// encrypted under the master key.
#[derive(Clone)]
pub struct Secrets {
    pool: SqlitePool,
    key: Arc<MasterKey>,
}

impl Secrets {
    #[must_use]
    pub fn new(store: &Store, key: Arc<MasterKey>) -> Self {
        Self {
            pool: store.pool.clone(),
            key,
        }
    }

    /// # Errors
    /// When the random source or the database write fails.
    pub async fn put(&self, name: &str, value: &str) -> Result<(), Error> {
        let sealed = self
            .key
            .seal(secret_context(name).as_bytes(), value.as_bytes())?;
        sqlx::query(
            "INSERT INTO secrets (name, sealed, updated_at) VALUES (?, ?, ?)
             ON CONFLICT (name) DO UPDATE SET sealed = excluded.sealed, updated_at = excluded.updated_at",
        )
        .bind(name)
        .bind(sealed)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// # Errors
    /// When the database read fails or the value can't be decrypted with this master key.
    pub async fn get(&self, name: &str) -> Result<Option<String>, Error> {
        let sealed: Option<Vec<u8>> =
            sqlx::query_scalar("SELECT sealed FROM secrets WHERE name = ?")
                .bind(name)
                .fetch_optional(&self.pool)
                .await?;
        sealed
            .map(|sealed| {
                let plain = self.key.open(secret_context(name).as_bytes(), &sealed)?;
                String::from_utf8(plain)
                    .map_err(|_| Error::Corrupt(format!("secret {name} is not text")))
            })
            .transpose()
    }

    /// # Errors
    /// When the database write fails.
    pub async fn delete(&self, name: &str) -> Result<(), Error> {
        sqlx::query("DELETE FROM secrets WHERE name = ?")
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn secret_context(name: &str) -> String {
    format!("secret:{name}")
}

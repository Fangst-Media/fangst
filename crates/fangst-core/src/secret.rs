//! Secrets at rest: the master key file, sealed values, password hashes and random tokens.
//!
//! The master key never leaves this module in any printable form. Two subkeys are derived from it
//! so that the key used to encrypt values is never also used to compute lookup hashes.

use std::fmt;
use std::io::{self, Write as _};
use std::path::Path;

use argon2::Argon2;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
/// Random bytes in a session token or generated API key; 256 bits can't be guessed online.
const TOKEN_BYTES: usize = 32;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    /// The key file exists but is not a key this module wrote.
    BadKeyFile(String),
    /// A sealed value was changed, truncated or sealed with another key.
    Tampered,
    Random,
    Hash(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "master key file: {e}"),
            Self::BadKeyFile(why) => write!(f, "master key file is not usable: {why}"),
            Self::Tampered => f.write_str(
                "a stored secret could not be decrypted; it was changed or the master key is not the one it was stored with",
            ),
            Self::Random => f.write_str("the operating system's random source failed"),
            Self::Hash(e) => write!(f, "password hashing failed: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// The key every stored secret is encrypted under. Losing the file makes stored secrets unreadable.
#[derive(Clone)]
pub struct MasterKey {
    seal: [u8; KEY_LEN],
    lookup: [u8; KEY_LEN],
}

impl fmt::Debug for MasterKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MasterKey(..)")
    }
}

impl MasterKey {
    /// Reads the key file, or creates it with fresh random bytes when it doesn't exist.
    ///
    /// # Errors
    /// When the file can't be read or written, or holds something other than a key.
    pub fn load_or_create(path: &Path) -> Result<Self, Error> {
        match Self::load(path) {
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
                let bytes: [u8; KEY_LEN] = random_bytes()?;
                if publish(path, &bytes)? {
                    Ok(Self::from_bytes(&bytes))
                } else {
                    // Another process created the key first; its key is the one in use.
                    Self::load(path)
                }
            }
            other => other,
        }
    }

    fn load(path: &Path) -> Result<Self, Error> {
        let bytes = std::fs::read(path)?;
        let bytes: [u8; KEY_LEN] = bytes.as_slice().try_into().map_err(|_| {
            Error::BadKeyFile(format!(
                "{} holds {} bytes, expected {KEY_LEN}",
                path.display(),
                bytes.len()
            ))
        })?;
        Ok(Self::from_bytes(&bytes))
    }

    #[must_use]
    pub fn from_bytes(bytes: &[u8; KEY_LEN]) -> Self {
        Self {
            seal: derive(bytes, b"fangst seal v1"),
            lookup: derive(bytes, b"fangst lookup v1"),
        }
    }

    /// Encrypts `plaintext`. `context` names what the value is for; opening it under another
    /// context fails, so a ciphertext can't be moved to another row and read there.
    ///
    /// # Errors
    /// When the random source fails.
    pub fn seal(&self, context: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, Error> {
        let nonce: [u8; NONCE_LEN] = random_bytes()?;
        let cipher = self.cipher();
        let ciphertext = cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: plaintext,
                    aad: context,
                },
            )
            .map_err(|_| Error::Tampered)?;
        let mut sealed = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        sealed.extend_from_slice(&nonce);
        sealed.extend_from_slice(&ciphertext);
        Ok(sealed)
    }

    /// # Errors
    /// [`Error::Tampered`] when the value, its context or the key doesn't match.
    pub fn open(&self, context: &[u8], sealed: &[u8]) -> Result<Vec<u8>, Error> {
        if sealed.len() < NONCE_LEN {
            return Err(Error::Tampered);
        }
        let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
        let nonce: [u8; NONCE_LEN] = nonce.try_into().map_err(|_| Error::Tampered)?;
        self.cipher()
            .decrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: ciphertext,
                    aad: context,
                },
            )
            .map_err(|_| Error::Tampered)
    }

    /// A keyed hash for finding a stored secret by its value without storing the value. Without
    /// the key file, a copy of the database gives nothing to brute-force short API keys against.
    #[must_use]
    pub fn lookup_hash(&self, value: &[u8]) -> String {
        to_hex(&hmac(&self.lookup, value))
    }

    fn cipher(&self) -> XChaCha20Poly1305 {
        XChaCha20Poly1305::new(&self.seal.into())
    }
}

fn derive(master: &[u8; KEY_LEN], label: &[u8]) -> [u8; KEY_LEN] {
    hmac(master, label)
}

fn hmac(key: &[u8], value: &[u8]) -> [u8; KEY_LEN] {
    let mut mac =
        <Hmac<Sha256> as KeyInit>::new_from_slice(key).expect("HMAC takes keys of any length");
    mac.update(value);
    mac.finalize().into_bytes().into()
}

/// Writes the key to a temporary file, flushes it, then links it into place. A crash leaves either
/// no key file or a whole one, never a short one. Returns false when `path` appeared meanwhile.
fn publish(path: &Path, bytes: &[u8]) -> Result<bool, Error> {
    let name = path
        .file_name()
        .map_or_else(|| "key".into(), |n| n.to_string_lossy().into_owned());
    let temp = path.with_file_name(format!(".{name}.{}.tmp", to_hex(&random_bytes::<8>()?)));
    let written = write_private(&temp, bytes);
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    // A hard link refuses to replace an existing file, so two processes starting at once can't
    // each believe their own key won. Filesystems without links fall back to a rename.
    let placed = match std::fs::hard_link(&temp, path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(_) => std::fs::rename(&temp, path).map(|()| true),
    };
    let _ = std::fs::remove_file(&temp);
    let placed = placed?;
    sync_parent(path);
    Ok(placed)
}

#[cfg(unix)]
fn sync_parent(path: &Path) {
    if let Some(parent) = path.parent()
        && let Ok(dir) = std::fs::File::open(parent)
    {
        let _ = dir.sync_all();
    }
}

// Windows has no handle to flush a directory entry with; NTFS journals the link itself.
#[cfg(not(unix))]
fn sync_parent(_path: &Path) {}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

// Windows files inherit the ACL of their folder; restricting it further needs the Win32 security
// API, so the data folder's own permissions are what protect the key there.
#[cfg(not(unix))]
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

fn random_bytes<const N: usize>() -> Result<[u8; N], Error> {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).map_err(|_| Error::Random)?;
    Ok(bytes)
}

/// A random token for sessions and generated API keys, as lowercase hex.
///
/// # Errors
/// When the random source fails.
pub fn random_token() -> Result<String, Error> {
    Ok(to_hex(&random_bytes::<TOKEN_BYTES>()?))
}

/// SHA-256 as hex. Fine for high-entropy tokens; use [`MasterKey::lookup_hash`] for anything a
/// person chose.
#[must_use]
pub fn sha256_hex(value: &[u8]) -> String {
    to_hex(&Sha256::digest(value))
}

/// An Argon2id hash in PHC form, with the crate's default cost (19 MiB, 2 passes, 1 lane).
///
/// # Errors
/// When the random source or the hash fails.
pub fn hash_password(password: &str) -> Result<String, Error> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|e| Error::Hash(e.to_string()))
}

/// False for a wrong password and for a hash that doesn't parse.
#[must_use]
pub fn verify_password(hash: &str, password: &str) -> bool {
    Argon2::default()
        .verify_password(password.as_bytes(), hash)
        .is_ok()
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> MasterKey {
        MasterKey::from_bytes(&[7; KEY_LEN])
    }

    #[test]
    fn sealed_values_open_only_with_the_same_key_and_context() {
        let sealed = key().seal(b"server:1", b"not-a-real-password").unwrap();
        assert!(
            !sealed
                .windows(b"not-a-real-password".len())
                .any(|w| w == b"not-a-real-password")
        );
        assert_eq!(
            key().open(b"server:1", &sealed).unwrap(),
            b"not-a-real-password"
        );
        assert!(matches!(
            key().open(b"server:2", &sealed),
            Err(Error::Tampered)
        ));
        let other = MasterKey::from_bytes(&[8; KEY_LEN]);
        assert!(matches!(
            other.open(b"server:1", &sealed),
            Err(Error::Tampered)
        ));
        let mut flipped = sealed.clone();
        *flipped.last_mut().unwrap() ^= 1;
        assert!(matches!(
            key().open(b"server:1", &flipped),
            Err(Error::Tampered)
        ));
    }

    #[test]
    fn sealing_twice_gives_different_ciphertexts() {
        assert_ne!(
            key().seal(b"c", b"same").unwrap(),
            key().seal(b"c", b"same").unwrap()
        );
    }

    #[test]
    fn key_file_is_created_once_and_reused() {
        let dir = std::env::temp_dir().join(format!("fangst-key-{}", random_token().unwrap()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("master.key");
        let first = MasterKey::load_or_create(&path).unwrap();
        assert_eq!(std::fs::read(&path).unwrap().len(), KEY_LEN);
        let second = MasterKey::load_or_create(&path).unwrap();
        let sealed = first.seal(b"c", b"v").unwrap();
        assert_eq!(second.open(b"c", &sealed).unwrap(), b"v");
        assert_eq!(format!("{first:?}"), "MasterKey(..)");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }

        std::fs::write(&path, b"short").unwrap();
        assert!(matches!(
            MasterKey::load_or_create(&path),
            Err(Error::BadKeyFile(_))
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn processes_starting_together_agree_on_one_key_and_leave_no_temp_files() {
        let dir = std::env::temp_dir().join(format!("fangst-key-{}", random_token().unwrap()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("master.key");
        let keys: Vec<MasterKey> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..8)
                .map(|_| s.spawn(|| MasterKey::load_or_create(&path).unwrap()))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let sealed = keys[0].seal(b"c", b"v").unwrap();
        for key in &keys {
            assert_eq!(key.open(b"c", &sealed).unwrap(), b"v");
        }
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["master.key"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn lookup_hash_depends_on_the_key() {
        assert_eq!(key().lookup_hash(b"k"), key().lookup_hash(b"k"));
        assert_ne!(
            key().lookup_hash(b"k"),
            MasterKey::from_bytes(&[8; KEY_LEN]).lookup_hash(b"k")
        );
    }

    #[test]
    fn passwords_hash_with_argon2id_and_verify() {
        let hash = hash_password("correct horse").unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password(&hash, "correct horse"));
        assert!(!verify_password(&hash, "wrong horse"));
        assert!(!verify_password("not a hash", "correct horse"));
    }

    #[test]
    fn tokens_are_random_hex() {
        let token = random_token().unwrap();
        assert_eq!(token.len(), TOKEN_BYTES * 2);
        assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(token, random_token().unwrap());
    }
}

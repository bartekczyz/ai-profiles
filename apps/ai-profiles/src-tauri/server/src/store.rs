//! Paired clients and pending pairings, in the state folder.
//!
//! Neither file holds a usable secret: a client's token and a pairing
//! secret are kept only as SHA-256. They are random and long, so a slow hash
//! would add nothing. Every change happens under `state.lock` and lands by
//! rename, so the CLI (`pair`, `revoke`) and the daemon can both write.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

const CLIENTS: &str = "clients.json";
const PENDING: &str = "pending-pairings.json";
const LOCK: &str = "state.lock";

/// How long a pairing code works for.
pub const PAIRING_TTL: Duration = Duration::minutes(10);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Client {
    pub id: String,
    pub name: String,
    pub token_sha256: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pending {
    secret_sha256: String,
    label: Option<String>,
    expires_at: DateTime<Utc>,
}

/// `len` random bytes, base64url.
pub fn random_token(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    getrandom::fill(&mut bytes).expect("the OS random source is available");
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Constant-time, so how much of a guess matched can't be timed.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(state_dir: &Path) -> Store {
        Store {
            dir: state_dir.to_path_buf(),
        }
    }

    /// Hold the state lock for as long as the guard lives.
    fn lock(&self) -> io::Result<File> {
        fs::create_dir_all(&self.dir)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(self.dir.join(LOCK))?;
        file.lock()?;
        Ok(file)
    }

    fn read<T: DeserializeOwned + Default>(&self, name: &str) -> io::Result<T> {
        match fs::read_to_string(self.dir.join(name)) {
            Ok(text) => serde_json::from_str(&text).map_err(io::Error::other),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(T::default()),
            Err(err) => Err(err),
        }
    }

    fn write<T: Serialize>(&self, name: &str, value: &T) -> io::Result<()> {
        let path = self.dir.join(name);
        let staged = self.dir.join(format!(".{name}.tmp"));
        fs::write(&staged, serde_json::to_vec_pretty(value)?)?;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o600))?;
        fs::rename(staged, path)
    }

    /// Remember a pairing secret for [`PAIRING_TTL`]. Expired ones are dropped
    /// on the way.
    pub fn add_pending(&self, secret: &str, label: Option<String>) -> io::Result<()> {
        let _lock = self.lock()?;
        let now = Utc::now();
        let mut pending: Vec<Pending> = self.read(PENDING)?;
        pending.retain(|entry| entry.expires_at > now);
        pending.push(Pending {
            secret_sha256: sha256_hex(secret),
            label,
            expires_at: now + PAIRING_TTL,
        });
        self.write(PENDING, &pending)
    }

    /// Use up a pairing secret, making a client of it. `None` when the secret
    /// is unknown, used or expired: the caller says only "not valid", so a
    /// guesser learns nothing about which.
    pub fn redeem(&self, secret: &str, client_name: &str) -> io::Result<Option<(Client, String)>> {
        let _lock = self.lock()?;
        let now = Utc::now();
        let hash = sha256_hex(secret);
        let mut pending: Vec<Pending> = self.read(PENDING)?;
        pending.retain(|entry| entry.expires_at > now);
        let found = pending
            .iter()
            .position(|entry| same(&entry.secret_sha256, &hash));
        let Some(index) = found else {
            self.write(PENDING, &pending)?;
            return Ok(None);
        };
        let entry = pending.remove(index);
        self.write(PENDING, &pending)?;

        let token = format!("aip_{}", random_token(32));
        let name = entry
            .label
            .filter(|label| !label.trim().is_empty())
            .unwrap_or_else(|| client_name.trim().chars().take(64).collect());
        let client = Client {
            id: uuid::Uuid::new_v4().to_string(),
            name: if name.is_empty() {
                "client".into()
            } else {
                name
            },
            token_sha256: sha256_hex(&token),
            created_at: now,
        };
        let mut clients: Vec<Client> = self.read(CLIENTS)?;
        clients.push(client.clone());
        self.write(CLIENTS, &clients)?;
        Ok(Some((client, token)))
    }

    pub fn clients(&self) -> io::Result<Vec<Client>> {
        self.read(CLIENTS)
    }

    /// The client a bearer token belongs to. Every stored hash is compared,
    /// so the time taken doesn't say which client was close.
    pub fn client_for_token(&self, token: &str) -> io::Result<Option<Client>> {
        let hash = sha256_hex(token);
        let mut found = None;
        for client in self.clients()? {
            if same(&client.token_sha256, &hash) && found.is_none() {
                found = Some(client);
            }
        }
        Ok(found)
    }

    /// Remove the client with this id or name. Returns the removed clients.
    pub fn revoke(&self, id_or_name: &str) -> io::Result<Vec<Client>> {
        let _lock = self.lock()?;
        let mut clients: Vec<Client> = self.read(CLIENTS)?;
        let (removed, kept): (Vec<_>, Vec<_>) = clients
            .drain(..)
            .partition(|client| client.id == id_or_name || client.name == id_or_name);
        self.write(CLIENTS, &kept)?;
        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pairing_secret_works_once_and_makes_a_client() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        store.add_pending("secret-1", Some("mac".into())).unwrap();

        let (client, token) = store.redeem("secret-1", "ignored").unwrap().unwrap();
        assert_eq!(client.name, "mac");
        assert!(token.starts_with("aip_"));
        assert_eq!(
            store.client_for_token(&token).unwrap(),
            Some(client.clone())
        );
        assert!(store.redeem("secret-1", "again").unwrap().is_none());
        assert!(store.redeem("wrong", "x").unwrap().is_none());

        // Nothing on disk can be used as a token or a secret.
        let on_disk = fs::read_to_string(dir.path().join(CLIENTS)).unwrap();
        assert!(!on_disk.contains(&token));
        let mode = fs::metadata(dir.path().join(CLIENTS))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn an_expired_secret_no_longer_works() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        store
            .write(
                PENDING,
                &vec![Pending {
                    secret_sha256: sha256_hex("old"),
                    label: None,
                    expires_at: Utc::now() - Duration::seconds(1),
                }],
            )
            .unwrap();
        assert!(store.redeem("old", "mac").unwrap().is_none());
    }

    #[test]
    fn revoking_by_id_or_name_stops_the_token_working() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        store.add_pending("a", None).unwrap();
        store.add_pending("b", None).unwrap();
        let (first, first_token) = store.redeem("a", "laptop").unwrap().unwrap();
        let (_, second_token) = store.redeem("b", "desktop").unwrap().unwrap();

        assert_eq!(store.revoke(&first.id).unwrap().len(), 1);
        assert_eq!(store.client_for_token(&first_token).unwrap(), None);
        assert_eq!(store.revoke("desktop").unwrap().len(), 1);
        assert_eq!(store.client_for_token(&second_token).unwrap(), None);
        assert!(store.revoke("nobody").unwrap().is_empty());
    }
}

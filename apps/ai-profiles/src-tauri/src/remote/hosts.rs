//! The remote hosts this Mac has paired with, in `remote-hosts.json` beside
//! `profiles.json`. Their tokens are not here: they live in the Keychain
//! (see [`super::secrets`]).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteHost {
    /// Ours, not the server's: a host keeps its id if its addresses change.
    pub id: String,
    /// What the sidebar calls it. The server's host name unless renamed.
    pub label: String,
    pub hostname: String,
    /// `host:port` candidates from the pairing code, best first.
    pub addresses: Vec<String>,
    /// SHA-256 of the server's certificate, lowercase hex: the only
    /// certificate this host is trusted with.
    pub fingerprint: String,
    /// What the server calls this Mac, for `ai-profiles-server revoke`.
    pub client_id: String,
    pub paired_at: String,
    /// The address that last answered, tried first next time.
    #[serde(default)]
    pub last_good_address: Option<String>,
    /// How this Mac shows each of the host's accounts as a profile, by
    /// account name. Only the look lives here: the account itself, its
    /// sign-in and sessions, are the host's.
    #[serde(default)]
    pub profiles: std::collections::BTreeMap<String, RemoteProfileLook>,
}

/// How a remote profile looks in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteProfileLook {
    /// `#rrggbb`, as local profiles have.
    pub color: String,
}

impl RemoteHost {
    /// Addresses in the order to try them: the one that last worked first.
    pub fn addresses_to_try(&self) -> Vec<String> {
        let mut ordered: Vec<String> = self.last_good_address.iter().cloned().collect();
        ordered.extend(
            self.addresses
                .iter()
                .filter(|address| Some(*address) != self.last_good_address.as_ref())
                .cloned(),
        );
        ordered
    }
}

/// Held across a load, change and save, so two commands can't each drop the
/// other's change.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct HostList {
    path: PathBuf,
}

impl HostList {
    pub fn at(path: PathBuf) -> HostList {
        HostList { path }
    }

    /// The app's own list.
    pub fn default_list() -> AppResult<HostList> {
        Ok(HostList::at(
            crate::paths::app_data_dir()?.join("remote-hosts.json"),
        ))
    }

    pub fn load(&self) -> AppResult<Vec<RemoteHost>> {
        match fs::read_to_string(&self.path) {
            Ok(text) if text.trim().is_empty() => Ok(Vec::new()),
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(err) => Err(err.into()),
        }
    }

    pub fn find(&self, id: &str) -> AppResult<RemoteHost> {
        self.load()?
            .into_iter()
            .find(|host| host.id == id)
            .ok_or_else(|| AppError::NotFound(format!("remote host {id} not found")))
    }

    pub fn add(&self, host: RemoteHost) -> AppResult<()> {
        let _guard = lock();
        let mut hosts = self.load()?;
        hosts.push(host);
        self.save(&hosts)
    }

    /// Change one host. Returns it as saved.
    pub fn update(&self, id: &str, change: impl FnOnce(&mut RemoteHost)) -> AppResult<RemoteHost> {
        let _guard = lock();
        let mut hosts = self.load()?;
        let host = hosts
            .iter_mut()
            .find(|host| host.id == id)
            .ok_or_else(|| AppError::NotFound(format!("remote host {id} not found")))?;
        change(host);
        let updated = host.clone();
        self.save(&hosts)?;
        Ok(updated)
    }

    pub fn remove(&self, id: &str) -> AppResult<Option<RemoteHost>> {
        let _guard = lock();
        let mut hosts = self.load()?;
        let removed = hosts
            .iter()
            .position(|host| host.id == id)
            .map(|index| hosts.remove(index));
        self.save(&hosts)?;
        Ok(removed)
    }

    fn save(&self, hosts: &[RemoteHost]) -> AppResult<()> {
        atomic_write(&self.path, &serde_json::to_vec_pretty(hosts)?)
    }
}

fn atomic_write(path: &Path, body: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::NotFound(format!("path {} has no parent", path.display())))?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(".remote-hosts.json.tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(body)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(id: &str) -> RemoteHost {
        RemoteHost {
            id: id.into(),
            label: "xjopa1".into(),
            hostname: "xjopa1".into(),
            addresses: vec!["100.1.2.3:7443".into(), "xjopa1:7443".into()],
            fingerprint: "ab".repeat(32),
            client_id: "c".into(),
            paired_at: "2026-09-22T00:00:00Z".into(),
            last_good_address: None,
            profiles: Default::default(),
        }
    }

    #[test]
    fn adds_updates_and_removes_hosts() {
        let dir = tempfile::tempdir().unwrap();
        let list = HostList::at(dir.path().join("remote-hosts.json"));
        assert!(list.load().unwrap().is_empty());
        list.add(host("a")).unwrap();
        list.add(host("b")).unwrap();
        let renamed = list
            .update("a", |host| host.label = "Linux box".into())
            .unwrap();
        assert_eq!(renamed.label, "Linux box");
        assert_eq!(list.find("a").unwrap().label, "Linux box");
        assert_eq!(list.remove("b").unwrap().map(|h| h.id), Some("b".into()));
        assert_eq!(list.remove("b").unwrap(), None);
        assert_eq!(list.load().unwrap().len(), 1);
        assert!(list.find("b").is_err());
    }

    #[test]
    fn tries_the_address_that_last_worked_first() {
        let mut host = host("a");
        assert_eq!(
            host.addresses_to_try(),
            vec!["100.1.2.3:7443", "xjopa1:7443"]
        );
        host.last_good_address = Some("xjopa1:7443".into());
        assert_eq!(
            host.addresses_to_try(),
            vec!["xjopa1:7443", "100.1.2.3:7443"]
        );
    }
}

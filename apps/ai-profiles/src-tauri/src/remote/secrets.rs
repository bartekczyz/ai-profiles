//! Where a remote host's token is kept: the login Keychain, as a generic
//! password (service `app.ai-profiles.remote-host`, account = the host's id).
//!
//! Written through `/usr/bin/security`, as the rest of the app reads Claude's
//! credentials, rather than a keyring crate. An item a crate creates trusts
//! the binary that created it, and a locally rebuilt (ad-hoc signed) app is a
//! new binary every time, so each rebuild would ask for Keychain access; an
//! item `security` creates trusts `security`, which doesn't change. The token
//! goes in on stdin (`security -i`), never in an argument a process listing
//! could show.

use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};

use crate::error::{AppError, AppResult};

const SERVICE: &str = "app.ai-profiles.remote-host";

pub trait SecretStore: Send + Sync {
    fn get(&self, host_id: &str) -> AppResult<Option<String>>;
    fn set(&self, host_id: &str, token: &str) -> AppResult<()>;
    fn delete(&self, host_id: &str) -> AppResult<()>;
}

/// The store the app uses: the Keychain, or memory under test.
pub fn store() -> &'static dyn SecretStore {
    static STORE: OnceLock<Box<dyn SecretStore>> = OnceLock::new();
    STORE
        .get_or_init(|| {
            if cfg!(test) {
                Box::new(MemoryStore::default())
            } else {
                Box::new(KeychainStore::default())
            }
        })
        .as_ref()
}

/// Tokens read once are kept in memory, so listing a host's sessions every
/// few seconds doesn't run `security` every time.
#[derive(Default)]
pub struct KeychainStore {
    cache: Mutex<HashMap<String, String>>,
}

impl SecretStore for KeychainStore {
    fn get(&self, host_id: &str) -> AppResult<Option<String>> {
        if let Some(token) = self
            .cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(host_id).cloned())
        {
            return Ok(Some(token));
        }
        let output = Command::new("/usr/bin/security")
            .args(["find-generic-password", "-s", SERVICE, "-a", host_id, "-w"])
            .stderr(Stdio::null())
            .output()?;
        if !output.status.success() {
            return Ok(None);
        }
        let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if token.is_empty() {
            return Ok(None);
        }
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(host_id.to_owned(), token.clone());
        }
        Ok(Some(token))
    }

    fn set(&self, host_id: &str, token: &str) -> AppResult<()> {
        // `security -i` reads commands as a shell would: the values must be
        // plain words. The id is a UUID and the token is `aip_` + base64url,
        // so this holds; refuse anything else rather than quote it.
        let plain = |text: &str| {
            !text.is_empty()
                && text
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        };
        if !plain(host_id) || !plain(token) {
            return Err(AppError::Validation(
                "unexpected characters in a token".into(),
            ));
        }
        let mut child = Command::new("/usr/bin/security")
            .arg("-i")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        child.stdin.take().expect("stdin is piped").write_all(
            format!("add-generic-password -U -s {SERVICE} -a {host_id} -w {token}\n").as_bytes(),
        )?;
        let output = child.wait_with_output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() || stderr.contains("error") {
            return Err(AppError::Io(std::io::Error::other(format!(
                "could not save the token in the Keychain: {}",
                stderr.trim()
            ))));
        }
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(host_id.to_owned(), token.to_owned());
        }
        Ok(())
    }

    fn delete(&self, host_id: &str) -> AppResult<()> {
        if let Ok(mut cache) = self.cache.lock() {
            cache.remove(host_id);
        }
        // Missing is fine: the goal is that it isn't there.
        let _ = Command::new("/usr/bin/security")
            .args(["delete-generic-password", "-s", SERVICE, "-a", host_id])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        Ok(())
    }
}

#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, host_id: &str) -> AppResult<Option<String>> {
        Ok(self.0.lock().ok().and_then(|map| map.get(host_id).cloned()))
    }

    fn set(&self, host_id: &str, token: &str) -> AppResult<()> {
        if let Ok(mut map) = self.0.lock() {
            map.insert(host_id.to_owned(), token.to_owned());
        }
        Ok(())
    }

    fn delete(&self, host_id: &str) -> AppResult<()> {
        if let Ok(mut map) = self.0.lock() {
            map.remove(host_id);
        }
        Ok(())
    }
}

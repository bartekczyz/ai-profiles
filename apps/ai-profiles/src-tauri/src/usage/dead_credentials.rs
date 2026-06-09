use std::collections::HashSet;
use std::sync::Mutex;

use sha2::{Digest, Sha256};

/// Tracks Claude profiles whose stored credentials cannot be auto-refreshed
/// (a dead refresh token — the CLI itself returns "Please run /login"). Once a
/// token is marked dead, the usage layer stops issuing network requests and
/// `claude` refresh spawns for it; those would only feed Anthropic's abuse
/// rate-limiter with invalid-auth attempts. Entries are keyed by a hash of the
/// access token, so re-authenticating (which rotates the token) is seen as a
/// fresh, untracked credential and polling resumes automatically.
#[derive(Default)]
pub struct DeadCredentialRegistry {
    dead: Mutex<HashSet<String>>,
}

impl DeadCredentialRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when this token was previously marked unrecoverable.
    pub fn is_dead(&self, token: &str) -> bool {
        self.dead.lock().unwrap().contains(&hash(token))
    }

    /// Records that this token needs a fresh interactive sign-in.
    pub fn mark_dead(&self, token: &str) {
        self.dead.lock().unwrap().insert(hash(token));
    }
}

/// SHA-256 hex of the token, so the raw secret is never retained in memory.
/// This set is only ever compared against itself, so the hash is independent
/// of the quota cache's key.
fn hash(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmarked_token_is_not_dead() {
        let registry = DeadCredentialRegistry::new();
        assert!(!registry.is_dead("sk-alive"));
    }

    #[test]
    fn marked_token_is_dead() {
        let registry = DeadCredentialRegistry::new();
        registry.mark_dead("sk-dead");
        assert!(registry.is_dead("sk-dead"));
    }

    #[test]
    fn tokens_are_tracked_independently() {
        let registry = DeadCredentialRegistry::new();
        registry.mark_dead("sk-dead");
        assert!(registry.is_dead("sk-dead"));
        assert!(!registry.is_dead("sk-other"));
    }

    #[test]
    fn a_rotated_token_is_seen_as_fresh() {
        // Re-auth changes the token string -> different hash -> not dead.
        // This is how polling auto-resumes after the user signs in again.
        let registry = DeadCredentialRegistry::new();
        registry.mark_dead("sk-old");
        assert!(!registry.is_dead("sk-new"));
    }
}

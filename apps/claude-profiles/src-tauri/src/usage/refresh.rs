//! Triggers Claude Code's built-in OAuth token refresh by spawning the
//! `claude` binary as a subprocess with the profile's `CLAUDE_CONFIG_DIR`
//! set and stdin closed.
//!
//! ## Why we delegate to Claude Code instead of refreshing ourselves
//!
//! The credentials blob stored by Claude Code includes a long-lived
//! `refreshToken` alongside the short-lived `accessToken`. Refreshing the
//! access token requires POSTing to Anthropic's OAuth token endpoint with
//! the refresh token and a `client_id` — neither of which is publicly
//! documented. Reverse-engineering them would couple us to undocumented
//! internals that can change without notice. Delegating to Claude Code
//! itself sidesteps that entire problem: when invoked it silently
//! refreshes its own token using its own knowledge of those endpoints.
//!
//! ## How the spawn works
//!
//! `claude < /dev/null > /dev/null 2>&1` exits cleanly within a few
//! hundred milliseconds — the REPL detects EOF on stdin during its
//! startup pass and exits before showing a prompt. The auth refresh
//! happens early in that startup pass, before stdin is read. We bound
//! the wait at 8s (matching the quota fetch timeout) and `kill_on_drop`
//! ensures the child is reaped if our timeout fires.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::Mutex as TokioMutex;

/// How long we'll wait for the spawned `claude` to finish refreshing
/// its token before giving up. Matches the quota fetch timeout.
const REFRESH_TIMEOUT: Duration = Duration::from_secs(8);

/// After a refresh attempt completes, skip further attempts for the
/// same profile inside this window. Without this, a profile whose
/// refresh token is permanently invalid would spawn `claude` on every
/// 5-minute refetch tick — many subprocesses per workday for no gain.
const REFRESH_BACKOFF: Duration = Duration::from_secs(60);

#[async_trait]
pub trait CliRefresher: Send + Sync {
    /// Best-effort: trigger a token refresh for the given profile config dir.
    /// Always returns — success is verified by the caller re-issuing the
    /// quota fetch, never by inspecting any return value here.
    async fn try_refresh(&self, cli_config_dir: &Path);
}

/// Tracks per-profile refresh state across calls so:
///   - two concurrent refreshes on the same profile serialise (the
///     second waits for the first instead of racing the same refresh
///     token through Anthropic's OAuth endpoint twice in parallel);
///   - a profile that just attempted a refresh is skipped for
///     `REFRESH_BACKOFF` so a permanently-broken profile doesn't spawn
///     `claude` on every refetch tick.
#[derive(Default)]
struct RefreshRegistry {
    inflight: StdMutex<HashMap<PathBuf, Arc<TokioMutex<()>>>>,
    last_attempt: StdMutex<HashMap<PathBuf, Instant>>,
}

impl RefreshRegistry {
    fn new() -> Self {
        Self::default()
    }

    /// Returns the async mutex for `key`, creating it if needed. The
    /// caller awaits `.lock()` on the returned mutex; concurrent calls
    /// on the same `key` serialise on it.
    fn slot(&self, key: &Path) -> Arc<TokioMutex<()>> {
        let mut map = self.inflight.lock().unwrap();
        map.entry(key.to_path_buf())
            .or_insert_with(|| Arc::new(TokioMutex::new(())))
            .clone()
    }

    fn should_skip(&self, key: &Path) -> bool {
        let map = self.last_attempt.lock().unwrap();
        match map.get(key) {
            Some(at) => at.elapsed() < REFRESH_BACKOFF,
            None => false,
        }
    }

    fn mark_attempted(&self, key: &Path) {
        let mut map = self.last_attempt.lock().unwrap();
        map.insert(key.to_path_buf(), Instant::now());
    }
}

/// Production refresher. Spawns the real `claude` binary, with a
/// per-profile mutex + 60 s backoff so it never races itself or
/// hammers a permanently-broken profile.
pub struct ClaudeCliRefresher {
    registry: RefreshRegistry,
}

impl ClaudeCliRefresher {
    pub fn new() -> Self {
        Self {
            registry: RefreshRegistry::new(),
        }
    }
}

impl Default for ClaudeCliRefresher {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl CliRefresher for ClaudeCliRefresher {
    async fn try_refresh(&self, cli_config_dir: &Path) {
        let slot = self.registry.slot(cli_config_dir);
        let _guard = slot.lock().await;
        // Re-check the backoff *after* acquiring the lock — another task
        // may have just finished a refresh on this profile while we were
        // waiting; if so we want to inherit its result, not re-spawn.
        if self.registry.should_skip(cli_config_dir) {
            return;
        }
        self.registry.mark_attempted(cli_config_dir);
        spawn_claude(cli_config_dir).await;
    }
}

async fn spawn_claude(cli_config_dir: &Path) {
    let Some(binary) = find_claude_binary() else {
        return;
    };
    let mut command = tokio::process::Command::new(&binary);
    // For the stock default profile we deliberately do NOT set
    // CLAUDE_CONFIG_DIR. Setting it — even to its implicit default
    // (`$HOME/.claude`) — flips Claude Code's keychain layout from the
    // bare `Claude Code-credentials` entry to the hashed
    // `Claude Code-credentials-<sha256(dir)[:8]>` form. The refreshed
    // token would land in the hashed entry while we keep reading from
    // bare, leaving the next quota fetch unauthorised again.
    if !crate::usage::is_stock_default_cli_config_dir(cli_config_dir) {
        command.env("CLAUDE_CONFIG_DIR", cli_config_dir);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let Ok(mut child) = command.spawn() else {
        return;
    };
    // We don't care about the exit status. If the timeout fires, the
    // future is dropped and `kill_on_drop` reaps the child.
    let _ = tokio::time::timeout(REFRESH_TIMEOUT, child.wait()).await;
}

/// Locate the `claude` binary. Tauri apps launched from Finder/Dock have
/// a minimal PATH (`/usr/bin:/bin:/usr/sbin:/sbin`) that excludes common
/// install locations, so PATH lookup alone is unreliable. We try PATH
/// first, then fall back to the standard install locations on macOS.
fn find_claude_binary() -> Option<PathBuf> {
    let path_env = std::env::var_os("PATH");
    let home = dirs::home_dir();
    find_claude_binary_in(path_env.as_deref(), home.as_deref())
}

/// Pure variant used by both `find_claude_binary` and tests. Takes the
/// raw `PATH` env value and the user's home dir as explicit inputs.
fn find_claude_binary_in(
    path_env: Option<&std::ffi::OsStr>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(found) = path_env.and_then(|path| find_in_path("claude", path)) {
        return Some(found);
    }
    let mut fallbacks: Vec<PathBuf> = Vec::new();
    if let Some(home) = home {
        fallbacks.push(home.join(".local").join("bin").join("claude"));
        fallbacks.push(home.join(".claude").join("local").join("claude"));
    }
    // `/opt/homebrew/bin` is Apple Silicon Homebrew specifically — only
    // probe it on macOS. `/usr/local/bin` is a common install location
    // on both macOS (Intel Homebrew) and Linux, so we keep it everywhere.
    #[cfg(target_os = "macos")]
    fallbacks.push(PathBuf::from("/opt/homebrew/bin/claude"));
    fallbacks.push(PathBuf::from("/usr/local/bin/claude"));
    fallbacks.into_iter().find(|candidate| candidate.is_file())
}

/// Pure: walk a PATH-style env value looking for the first entry that
/// contains an executable file named `name`.
fn find_in_path(name: &str, path_env: &std::ffi::OsStr) -> Option<PathBuf> {
    for dir in std::env::split_paths(path_env) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::TempDir;

    use super::*;

    fn make_executable(path: &Path) {
        fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
    }

    #[test]
    fn find_in_path_returns_first_match() {
        let first = TempDir::new().unwrap();
        let second = TempDir::new().unwrap();
        make_executable(&first.path().join("dummy-binary"));
        make_executable(&second.path().join("dummy-binary"));

        let combined = std::env::join_paths([first.path(), second.path()]).unwrap();
        let found = find_in_path("dummy-binary", &combined);
        assert_eq!(found.unwrap(), first.path().join("dummy-binary"));
    }

    #[test]
    fn find_in_path_returns_none_when_missing() {
        let dir = TempDir::new().unwrap();
        let path_env = std::ffi::OsString::from(dir.path());
        let result = find_in_path("definitely-not-here-9f86d081", &path_env);
        assert!(result.is_none());
    }

    #[test]
    fn find_claude_binary_in_prefers_path_over_fallbacks() {
        let path_dir = TempDir::new().unwrap();
        make_executable(&path_dir.path().join("claude"));
        let fake_home = TempDir::new().unwrap();
        // Also create a fallback so we can prove PATH wins.
        let fallback_dir = fake_home.path().join(".local").join("bin");
        fs::create_dir_all(&fallback_dir).unwrap();
        make_executable(&fallback_dir.join("claude"));

        let path_env = std::ffi::OsString::from(path_dir.path());
        let found = find_claude_binary_in(Some(&path_env), Some(fake_home.path()));
        assert_eq!(found.unwrap(), path_dir.path().join("claude"));
    }

    #[test]
    fn find_claude_binary_in_falls_back_to_home_when_path_misses() {
        let fake_home = TempDir::new().unwrap();
        let fallback_dir = fake_home.path().join(".local").join("bin");
        fs::create_dir_all(&fallback_dir).unwrap();
        let expected = fallback_dir.join("claude");
        make_executable(&expected);

        // Empty PATH-like input: a tempdir with no `claude` in it.
        let empty_path_dir = TempDir::new().unwrap();
        let path_env = std::ffi::OsString::from(empty_path_dir.path());
        let found = find_claude_binary_in(Some(&path_env), Some(fake_home.path()));
        assert_eq!(found.unwrap(), expected);
    }

    // No "returns None when nothing found" test — `/opt/homebrew/bin/claude`
    // and `/usr/local/bin/claude` are real paths that may exist on the
    // test runner and we don't want a system-state-dependent flake.

    // --- RefreshRegistry primitives ---

    #[test]
    fn registry_does_not_skip_first_attempt() {
        let registry = RefreshRegistry::new();
        let dir = PathBuf::from("/tmp/test-prof-fresh");
        assert!(!registry.should_skip(&dir));
    }

    #[test]
    fn registry_skips_within_backoff_window() {
        let registry = RefreshRegistry::new();
        let dir = PathBuf::from("/tmp/test-prof-recent");
        registry.mark_attempted(&dir);
        assert!(registry.should_skip(&dir));
    }

    #[test]
    fn registry_tracks_per_profile_independently() {
        let registry = RefreshRegistry::new();
        let dir_a = PathBuf::from("/tmp/test-prof-a");
        let dir_b = PathBuf::from("/tmp/test-prof-b");
        registry.mark_attempted(&dir_a);
        assert!(registry.should_skip(&dir_a));
        assert!(!registry.should_skip(&dir_b));
    }

    #[tokio::test]
    async fn registry_slot_serialises_same_profile() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let registry = Arc::new(RefreshRegistry::new());
        let counter = Arc::new(AtomicU32::new(0));
        let max_observed = Arc::new(AtomicU32::new(0));
        let dir = PathBuf::from("/tmp/test-prof-serialise");

        let mut tasks = Vec::new();
        for _ in 0..5 {
            let registry = registry.clone();
            let counter = counter.clone();
            let max_observed = max_observed.clone();
            let dir = dir.clone();
            tasks.push(tokio::spawn(async move {
                let slot = registry.slot(&dir);
                let _guard = slot.lock().await;
                let inflight = counter.fetch_add(1, Ordering::SeqCst) + 1;
                max_observed.fetch_max(inflight, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                counter.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        // At most one holder of the slot lock at any time.
        assert_eq!(max_observed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn registry_slot_does_not_serialise_different_profiles() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let registry = Arc::new(RefreshRegistry::new());
        let counter = Arc::new(AtomicU32::new(0));
        let max_observed = Arc::new(AtomicU32::new(0));

        let mut tasks = Vec::new();
        for index in 0..4 {
            let registry = registry.clone();
            let counter = counter.clone();
            let max_observed = max_observed.clone();
            let dir = PathBuf::from(format!("/tmp/test-prof-parallel-{index}"));
            tasks.push(tokio::spawn(async move {
                let slot = registry.slot(&dir);
                let _guard = slot.lock().await;
                let inflight = counter.fetch_add(1, Ordering::SeqCst) + 1;
                max_observed.fetch_max(inflight, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(20)).await;
                counter.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for task in tasks {
            task.await.unwrap();
        }
        // Different profiles can run concurrently, so we should see >1.
        assert!(max_observed.load(Ordering::SeqCst) > 1);
    }
}

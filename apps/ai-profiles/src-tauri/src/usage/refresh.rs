//! Triggers Claude Code's built-in OAuth token refresh by driving a real
//! interactive `claude` session under a pseudo-terminal far enough that it
//! rotates and persists its own token, then exiting.
//!
//! ## Why we delegate to Claude Code instead of refreshing ourselves
//!
//! The credentials blob stored by Claude Code includes a long-lived
//! `refreshToken` alongside the short-lived `accessToken`. Refreshing the
//! access token requires POSTing to Anthropic's OAuth token endpoint with
//! the refresh token and a `client_id` — neither of which is publicly
//! documented. Reverse-engineering them would couple us to undocumented
//! internals that can change without notice. Delegating to Claude Code
//! itself sidesteps that entire problem: when invoked interactively it
//! silently refreshes its own token using its own knowledge of those
//! endpoints, and — unlike `claude -p` — *persists* the rotated refresh
//! token back to the keychain.
//!
//! ## Why a pseudo-terminal, and why a prompt
//!
//! With a plain piped / `/dev/null` stdin, claude detects "no TTY" and treats
//! the invocation as `--print` mode, exiting with a usage error *before any
//! auth work*. So a non-pty spawn never refreshes anything. We allocate a real
//! pty (via `portable-pty`, so we own claude's pid and can reap it cleanly —
//! no orphaned children), wait for the REPL to finish starting, then send a
//! trivial prompt. The prompt forces an API interaction, which is what
//! reliably triggers the refresh + persist. We then wait (bounded by
//! [`REFRESH_TIMEOUT`]) for the stored token to change, send `/exit`, and kill
//! the child. Success is verified by the caller re-reading the token, never by
//! the child's exit status.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::Mutex as TokioMutex;

/// How long we'll wait for the spawned `claude` to persist a refreshed token
/// before giving up. Generous: a refresh behind a slow network can take a few
/// seconds, and the child is reaped cleanly either way.
const REFRESH_TIMEOUT: Duration = Duration::from_secs(30);

/// Quiet stretch with no new pty output that we take to mean the REPL has
/// finished starting and is ready for input. Sending the prompt earlier would
/// lose it into a not-yet-ready readline.
const SETTLE_QUIET: Duration = Duration::from_millis(500);

/// Ceiling on how long we wait for the REPL to settle before sending the
/// prompt anyway.
const SETTLE_CAP: Duration = Duration::from_secs(10);

/// How often we re-read the credential while waiting for the refresh to land.
const ROTATE_POLL: Duration = Duration::from_millis(150);

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
    // For the stock default profile we deliberately do NOT set
    // CLAUDE_CONFIG_DIR. Setting it — even to its implicit default
    // (`$HOME/.claude`) — flips Claude Code's keychain layout from the
    // bare `Claude Code-credentials` entry to the hashed
    // `Claude Code-credentials-<sha256(dir)[:8]>` form. The refreshed
    // token would land in the hashed entry while we keep reading from
    // bare, leaving the next quota fetch unauthorised again.
    let set_config = !crate::usage::is_stock_default_cli_config_dir(cli_config_dir);
    let dir = cli_config_dir.to_path_buf();
    // `portable-pty` is blocking; run the whole dance off the async runtime.
    let _ = tokio::task::spawn_blocking(move || run_pty_refresh(&binary, &dir, set_config)).await;
}

/// Pure: true once the pty has produced output and then gone quiet for
/// [`SETTLE_QUIET`], or the [`SETTLE_CAP`] ceiling has been reached. Used to
/// decide when the REPL is ready to receive the prompt.
fn prompt_is_ready(saw_output: bool, quiet_for: Duration, settle_elapsed: Duration) -> bool {
    if settle_elapsed >= SETTLE_CAP {
        return true;
    }
    saw_output && quiet_for >= SETTLE_QUIET
}

/// Drives a real interactive `claude` under a pty far enough to refresh +
/// persist its token, then exits. Best-effort: any failure just returns, and
/// the caller verifies success by re-reading the credential.
fn run_pty_refresh(binary: &Path, cli_config_dir: &Path, set_config: bool) {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};

    let Ok(pair) = native_pty_system().openpty(PtySize {
        rows: 40,
        cols: 120,
        pixel_width: 0,
        pixel_height: 0,
    }) else {
        return;
    };
    let portable_pty::PtyPair { master, slave } = pair;

    let mut cmd = CommandBuilder::new(binary);
    if set_config {
        cmd.env("CLAUDE_CONFIG_DIR", cli_config_dir);
    }
    if let Some(home) = dirs::home_dir() {
        cmd.cwd(home);
    }

    let Ok(mut child) = slave.spawn_command(cmd) else {
        return;
    };
    // The child holds the slave fd now; drop ours so EOF propagates on exit.
    drop(slave);

    let (Ok(mut reader), Ok(mut writer)) = (master.try_clone_reader(), master.take_writer()) else {
        let _ = child.kill();
        let _ = child.wait();
        return;
    };

    // Drain output on a thread — an unread pty fills its buffer and stalls the
    // child — while tracking when the last byte arrived so we can tell when
    // startup has settled.
    let last_byte = Arc::new(StdMutex::new(Instant::now()));
    let saw_output = Arc::new(AtomicBool::new(false));
    {
        let last_byte = Arc::clone(&last_byte);
        let saw_output = Arc::clone(&saw_output);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(read) = reader.read(&mut buf) {
                if read == 0 {
                    break;
                }
                *last_byte.lock().unwrap() = Instant::now();
                saw_output.store(true, Ordering::SeqCst);
            }
        });
    }

    // Wait for the REPL to settle before sending the prompt.
    let settle_start = Instant::now();
    while !prompt_is_ready(
        saw_output.load(Ordering::SeqCst),
        last_byte.lock().unwrap().elapsed(),
        settle_start.elapsed(),
    ) {
        std::thread::sleep(Duration::from_millis(50));
    }

    // A trivial prompt forces an API interaction → claude refreshes + persists
    // its token. `\r` is Enter inside a TTY.
    let token_before = crate::usage::credentials::read_access_token(cli_config_dir).ok();
    let _ = writer.write_all(b"hi\r");
    let _ = writer.flush();

    // Wait (bounded) for the persisted token to change.
    let rotate_deadline = Instant::now() + REFRESH_TIMEOUT;
    loop {
        let current = crate::usage::credentials::read_access_token(cli_config_dir).ok();
        if current != token_before || Instant::now() >= rotate_deadline {
            break;
        }
        std::thread::sleep(ROTATE_POLL);
    }

    // Clean shutdown first (lets claude reap its own children), then ensure it.
    let _ = writer.write_all(b"/exit\r");
    let _ = writer.flush();
    std::thread::sleep(Duration::from_millis(300));
    let _ = child.kill();
    let _ = child.wait();
    // `master` stays alive until here so the pty isn't torn down early.
    drop(master);
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

    // --- prompt_is_ready (pure) ---

    #[test]
    fn prompt_is_not_ready_before_any_output() {
        // No output yet → not ready, even after a quiet stretch.
        assert!(!prompt_is_ready(
            false,
            Duration::from_secs(2),
            Duration::from_secs(2)
        ));
    }

    #[test]
    fn prompt_is_not_ready_while_output_is_still_flowing() {
        // Output seen, but the last byte was too recent → REPL still starting.
        assert!(!prompt_is_ready(
            true,
            Duration::from_millis(100),
            Duration::from_secs(2)
        ));
    }

    #[test]
    fn prompt_is_ready_after_output_then_quiet() {
        assert!(prompt_is_ready(true, SETTLE_QUIET, Duration::from_secs(2)));
    }

    #[test]
    fn prompt_is_ready_at_the_settle_ceiling_regardless_of_output() {
        // Even if claude never produced output, stop waiting at the cap.
        assert!(prompt_is_ready(false, Duration::ZERO, SETTLE_CAP));
    }

    // --- manual end-to-end smoke (real claude + keychain) ---
    // Gated behind an env var pointing at a profile's cli-config dir, since it
    // spawns the real binary and makes one trivial API call. Run with:
    //   AI_PROFILES_PTY_SMOKE="<cli-config dir>" cargo test \
    //     --manifest-path apps/ai-profiles/src-tauri/Cargo.toml \
    //     usage::refresh::tests::pty_refresh_smoke -- --nocapture --ignored
    #[test]
    #[ignore = "spawns real claude; opt in via AI_PROFILES_PTY_SMOKE"]
    fn pty_refresh_smoke() {
        let Ok(dir) = std::env::var("AI_PROFILES_PTY_SMOKE") else {
            eprintln!("set AI_PROFILES_PTY_SMOKE=<cli-config dir> to run");
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let binary = find_claude_binary().expect("claude binary on PATH");
        let before = crate::usage::credentials::read_access_token(&dir).ok();
        let started = Instant::now();
        run_pty_refresh(&binary, &dir, true);
        let after = crate::usage::credentials::read_access_token(&dir).ok();
        eprintln!(
            "pty_refresh_smoke: elapsed={:?} rotated={}",
            started.elapsed(),
            before != after
        );
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

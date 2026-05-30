//! Single-instance launch handling for the Claude desktop app.
//!
//! Claude does NOT enforce one instance per data directory: `open -n` always
//! spawns a brand-new process, and two instances happily share the same
//! `--user-data-dir`. To give each entry (the stock default and every managed
//! profile) "exactly one window, all coexisting" behaviour, we track the
//! running instances ourselves: before launching we scan for a main Claude
//! process already bound to the target data dir, and focus it instead of
//! spawning a duplicate.

use std::process::Command;

use crate::error::{AppError, AppResult};

/// Find the PID of the *main* Claude process bound to `data_dir` in `ps`
/// output, or `None`.
///
/// Every running instance is one `…/Contents/MacOS/Claude
/// --user-data-dir=<dir>` process. Helper processes (renderer, GPU, network,
/// …) carry the same flag, but they run a different executable
/// (`…/MacOS/Claude Helper`) and always have trailing arguments. Requiring the
/// line to end with `…/Contents/MacOS/Claude --user-data-dir=<dir>` therefore
/// matches only the main process and rejects helpers, the crashpad handler,
/// and instances bound to any other data dir.
pub fn find_running_claude_pid(ps_output: &str, data_dir: &str) -> Option<i32> {
    let suffix = format!("/Contents/MacOS/Claude --user-data-dir={data_dir}");
    for line in ps_output.lines() {
        let Some((pid, command)) = line.trim_start().split_once(char::is_whitespace) else {
            continue;
        };
        if command.trim_end().ends_with(&suffix) {
            if let Ok(parsed) = pid.parse::<i32>() {
                return Some(parsed);
            }
        }
    }
    None
}

/// Scan running processes for a main Claude bound to `data_dir`.
fn running_claude_pid(data_dir: &str) -> AppResult<Option<i32>> {
    let output = Command::new("ps")
        .args(["-ax", "-o", "pid=,command="])
        .output()
        .map_err(AppError::Io)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(find_running_claude_pid(&stdout, data_dir))
}

/// Surface the already-running instance that owns `pid`: ask it to reopen a
/// window, then bring it to the foreground.
///
/// The reopen step matters because closing a Claude window does not quit the
/// app on macOS — the process keeps running with no windows. Plain activation
/// would bring that windowless process forward but show nothing. Sending it
/// the `reopen` Apple event (`aevt`/`rapp`) — exactly what clicking the Dock
/// icon does — makes it recreate its window. Addressing the event to a
/// specific PID keeps it scoped to the right instance even when several Claude
/// processes (other profiles) are running under the shared bundle id.
///
/// Best-effort: a missing process or a refused step is ignored — the caller
/// has already decided not to spawn a duplicate.
#[cfg(target_os = "macos")]
fn focus_pid(pid: i32) {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventSendOptions};

    // 'aevt'/'rapp' — the standard reopen-application event. Reopen is exempt
    // from the Automation (TCC) consent prompt, so this is silent.
    const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
    const REOPEN_APPLICATION: u32 = u32::from_be_bytes(*b"rapp");
    const AUTO_GENERATE_RETURN_ID: i16 = -1;
    const ANY_TRANSACTION_ID: i32 = 0;
    const TIMEOUT_SECONDS: f64 = 5.0;

    let target = NSAppleEventDescriptor::descriptorWithProcessIdentifier(pid);
    let reopen = NSAppleEventDescriptor::appleEventWithEventClass_eventID_targetDescriptor_returnID_transactionID(
        CORE_EVENT_CLASS,
        REOPEN_APPLICATION,
        Some(&target),
        AUTO_GENERATE_RETURN_ID,
        ANY_TRANSACTION_ID,
    );
    let _ = reopen
        .sendEventWithOptions_timeout_error(NSAppleEventSendOptions::NoReply, TIMEOUT_SECONDS);

    // Activating another application requires no entitlement and triggers no
    // TCC prompt.
    #[allow(deprecated)]
    if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    }
}

#[cfg(not(target_os = "macos"))]
fn focus_pid(_pid: i32) {}

/// Focus the running Claude bound to `data_dir`, or run `launch` when none is
/// running. This is the single-instance gate shared by the default entry and
/// the managed profiles.
pub fn focus_or_launch<F>(data_dir: &str, launch: F) -> AppResult<()>
where
    F: FnOnce() -> AppResult<()>,
{
    if let Some(pid) = running_claude_pid(data_dir)? {
        focus_pid(pid);
        return Ok(());
    }
    launch()
}

/// Launch a fresh Claude instance bound to `data_dir` via
/// `open -n -a "Claude" --args --user-data-dir=<dir>` — the same incantation
/// the per-profile launcher `.app` bundles use, just invoked directly. Used
/// by the default entry, which has no launcher bundle of its own.
pub fn open_new_instance(data_dir: &str) -> AppResult<()> {
    let status = Command::new("open")
        .arg("-n")
        .arg("-a")
        .arg("Claude")
        .arg("--args")
        .arg(format!("--user-data-dir={data_dir}"))
        .status()
        .map_err(AppError::Io)?;
    if !status.success() {
        return Err(AppError::Validation(format!(
            "`open -n -a Claude --args --user-data-dir={data_dir}` exited with status {status}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const STOCK_DIR: &str = "/Users/me/Library/Application Support/Claude";
    const PROFILE_DIR: &str =
        "/Users/me/Library/Application Support/claude-profiles/profiles/abc/gui-data";

    /// Mirrors real `ps -ax -o pid=,command=` output: leading-padded PIDs, a
    /// main process per instance, and helper/crashpad lines that also carry
    /// `--user-data-dir` but must NOT match.
    fn sample_ps() -> String {
        format!(
            "\
  54587 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={STOCK_DIR}
  56318 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={PROFILE_DIR}
  54590 /Applications/Claude.app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper --type=gpu-process --user-data-dir={STOCK_DIR} --gpu-preferences=xyz
   1866 /Applications/Claude.app/Contents/Frameworks/Electron Framework.framework/Helpers/chrome_crashpad_handler --database={STOCK_DIR}/Crashpad --handshake-fd=18
"
        )
    }

    #[test]
    fn matches_main_process_for_the_stock_data_dir() {
        assert_eq!(
            find_running_claude_pid(&sample_ps(), STOCK_DIR),
            Some(54587)
        );
    }

    #[test]
    fn matches_main_process_for_a_profile_data_dir() {
        assert_eq!(
            find_running_claude_pid(&sample_ps(), PROFILE_DIR),
            Some(56318)
        );
    }

    #[test]
    fn returns_none_when_no_instance_is_bound_to_the_dir() {
        let other = "/Users/me/Library/Application Support/claude-profiles/profiles/zzz/gui-data";
        assert_eq!(find_running_claude_pid(&sample_ps(), other), None);
    }

    #[test]
    fn ignores_helper_processes_that_share_the_data_dir() {
        // A renderer/GPU helper carries --user-data-dir but is not the main
        // process; if only helpers were running we must report nothing.
        let helpers_only = format!(
            "  54590 /Applications/Claude.app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper --type=renderer --user-data-dir={STOCK_DIR} --enable-sandbox\n"
        );
        assert_eq!(find_running_claude_pid(&helpers_only, STOCK_DIR), None);
    }

    #[test]
    fn does_not_let_the_stock_dir_match_a_profile_whose_path_extends_it() {
        // The stock dir is a path prefix of nothing here, but guard the
        // reverse: searching the stock dir must not match the longer profile
        // line, and vice versa.
        assert_ne!(
            find_running_claude_pid(&sample_ps(), STOCK_DIR),
            Some(56318)
        );
        assert_ne!(
            find_running_claude_pid(&sample_ps(), PROFILE_DIR),
            Some(54587)
        );
    }
}

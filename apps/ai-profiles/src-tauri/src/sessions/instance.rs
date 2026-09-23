//! The desktop app instance a home's sessions can be open in, and quitting it.

use std::collections::HashSet;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use super::Home;
use crate::error::{AppError, AppResult};
use crate::launch::{find_running_pids, pids_ending_with, process_list};

/// How long a desktop app is given to quit before an action gives up on it.
pub const QUIT_TIMEOUT: Duration = Duration::from_secs(10);

/// How often a quitting desktop app is looked for.
const QUIT_POLL: Duration = Duration::from_millis(200);

/// The pid of the main process of `home`'s desktop app instance, given the
/// output of `ps -ax -o pid=,command=`, if it runs: the first of
/// [`desktop_pids`].
pub fn desktop_pid(home: &Home, ps_output: &str) -> Option<i32> {
    desktop_pids(home, ps_output).into_iter().next()
}

/// The pids of the main processes of `home`'s desktop app instances, given
/// the output of `ps -ax -o pid=,command=`. Usually one, but the app doesn't
/// keep to one instance per data dir.
///
/// A profile's instance runs with `--user-data-dir=<its gui data dir>`, and so
/// does the stock one when ai-profiles started it (see [`find_running_pid`]).
/// The stock app started from the Dock or Finder runs from the vendor bundle
/// with no arguments at all, which also makes it the stock home's instance. A
/// profile's wrapper also runs without arguments while macOS starts it, but
/// from its own bundle, so it isn't taken for the stock one.
///
/// [`find_running_pid`]: crate::launch::find_running_pid
pub fn desktop_pids(home: &Home, ps_output: &str) -> Vec<i32> {
    let data_dir = home.gui_data_dir.display().to_string();
    let mut pids = Vec::new();
    for candidate in home.app.spec().gui_bundle_candidates {
        pids.extend(find_running_pids(
            ps_output,
            &data_dir,
            candidate.macos_exec,
        ));
        if home.stock {
            let bare = format!(
                "/{}/Contents/MacOS/{}",
                candidate.bundle_name, candidate.macos_exec
            );
            pids.extend(pids_ending_with(ps_output, &[bare]));
        }
    }
    let mut seen = HashSet::new();
    pids.retain(|pid| seen.insert(*pid));
    pids
}

/// The pid of the main process of `home`'s desktop app instance, if it runs.
pub fn running_desktop_pid(home: &Home) -> AppResult<Option<i32>> {
    Ok(desktop_pid(home, &process_list()?))
}

/// Quit `home`'s desktop app instances, if any run, and wait up to `timeout`
/// for them to be gone. Only that home's are asked: SIGTERM to each main
/// process, which quits the app as the Quit menu item does, and to any that
/// shows up while waiting. One still running at the deadline (it refused, or
/// kept starting again) is an error.
pub fn quit_desktop(home: &Home, timeout: Duration) -> AppResult<()> {
    let deadline = Instant::now() + timeout;
    let mut asked = HashSet::new();
    loop {
        let pids = desktop_pids(home, &process_list()?);
        if pids.is_empty() {
            return Ok(());
        }
        for pid in pids {
            if asked.insert(pid) {
                Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .status()?;
            }
        }
        if Instant::now() >= deadline {
            return Err(AppError::Validation(format!(
                "{} didn't quit",
                desktop_label(home)
            )));
        }
        thread::sleep(QUIT_POLL);
    }
}

/// How `home`'s desktop app instance is named to the user: `Claude (Work)`.
pub fn desktop_label(home: &Home) -> String {
    format!("{} ({})", home.app.spec().display_name, home.label)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Child;

    use tempfile::tempdir;

    use super::*;
    use crate::app_kind::AppKind;
    use crate::test_support::{fake_wrapper_process, stubborn_wrapper_process};

    const STOCK_DIR: &str = "/Users/me/Library/Application Support/Codex";
    const PROFILE_DIR: &str =
        "/Users/me/Library/Application Support/ai-profiles/profiles/abc/gui-data";

    fn home(gui_data_dir: &str, stock: bool) -> Home {
        Home {
            id: "abc".to_string(),
            app: AppKind::Codex,
            label: "Personal".to_string(),
            config_dir: PathBuf::from("/Users/me/.codex"),
            gui_data_dir: PathBuf::from(gui_data_dir),
            stock,
        }
    }

    #[test]
    fn a_profile_runs_when_an_instance_is_bound_to_its_data_dir() {
        let ps_output = format!(
            "  700 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT\n  \
             710 /Applications/ChatGPT (Personal).app/Contents/MacOS/ChatGPT.bin --user-data-dir={PROFILE_DIR}\n"
        );

        assert_eq!(
            desktop_pid(&home(PROFILE_DIR, false), &ps_output),
            Some(710)
        );
        assert_eq!(
            desktop_pid(
                &home(PROFILE_DIR, false),
                "  700 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT\n"
            ),
            None
        );
    }

    #[test]
    fn the_stock_home_runs_when_the_app_was_started_without_arguments() {
        let ps_output = "  \
             701 /Applications/ChatGPT.app/Contents/Frameworks/ChatGPT Helper.app/Contents/MacOS/ChatGPT Helper --type=gpu-process\n  \
             700 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT\n";

        assert_eq!(desktop_pid(&home(STOCK_DIR, true), ps_output), Some(700));
    }

    #[test]
    fn the_stock_home_runs_when_ai_profiles_started_it_on_its_data_dir() {
        let ps_output = format!(
            "  720 /Applications/Codex.app/Contents/MacOS/Codex --user-data-dir={STOCK_DIR}\n"
        );

        assert_eq!(desktop_pid(&home(STOCK_DIR, true), &ps_output), Some(720));
    }

    #[test]
    fn a_wrapper_macos_is_still_starting_is_not_the_stock_instance() {
        let ps_output = "  730 /Applications/ChatGPT (Personal).app/Contents/MacOS/ChatGPT\n";

        assert_eq!(desktop_pid(&home(STOCK_DIR, true), ps_output), None);
    }

    #[test]
    fn helpers_and_other_profiles_are_not_the_stock_instance() {
        let ps_output = format!(
            "  701 /Applications/ChatGPT.app/Contents/Frameworks/ChatGPT Helper.app/Contents/MacOS/ChatGPT Helper --type=renderer\n  \
             710 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT --user-data-dir={PROFILE_DIR}\n"
        );

        assert_eq!(desktop_pid(&home(STOCK_DIR, true), &ps_output), None);
    }

    /// A managed Claude home whose desktop app keeps its data in
    /// `<root>/<name>/gui-data`.
    fn claude_home(root: &Path, name: &str) -> Home {
        Home {
            id: name.to_string(),
            app: AppKind::Claude,
            label: name.to_string(),
            config_dir: root.join(name).join("cli-config"),
            gui_data_dir: root.join(name).join("gui-data"),
            stock: false,
        }
    }

    /// Reap `child` on another thread once it exits, as launchd reaps a real
    /// app, so it leaves the process list. Its stdin is kept open, as the
    /// stand-in exits when that closes.
    fn reaped(mut child: Child) -> (thread::JoinHandle<bool>, Option<std::process::ChildStdin>) {
        let stdin = child.stdin.take();
        let reaper = thread::spawn(move || child.wait().is_ok());
        (reaper, stdin)
    }

    #[test]
    fn quitting_a_homes_desktop_app_leaves_other_homes_running() {
        let root = tempdir().unwrap();
        let work = claude_home(root.path(), "Work");
        let side = claude_home(root.path(), "Side");
        let mut other = fake_wrapper_process(&root.path().join("side-app"), &side.gui_data_dir);
        let (reaper, _stdin) = reaped(fake_wrapper_process(
            &root.path().join("work-app"),
            &work.gui_data_dir,
        ));
        assert!(running_desktop_pid(&work).unwrap().is_some());

        let quit = quit_desktop(&work, QUIT_TIMEOUT);

        let gone = running_desktop_pid(&work).unwrap().is_none();
        let survived = other.try_wait().unwrap().is_none();
        other.kill().unwrap();
        other.wait().unwrap();
        quit.unwrap();
        assert!(gone, "the home's desktop app still runs");
        assert!(survived, "another home's desktop app was quit");
        assert!(reaper.join().unwrap());
    }

    #[test]
    fn every_instance_on_a_homes_data_dir_is_quit() {
        let root = tempdir().unwrap();
        let work = claude_home(root.path(), "Work");
        let (first, first_stdin) = reaped(fake_wrapper_process(
            &root.path().join("first-app"),
            &work.gui_data_dir,
        ));
        let (second, second_stdin) = reaped(fake_wrapper_process(
            &root.path().join("second-app"),
            &work.gui_data_dir,
        ));
        let running = desktop_pids(&work, &process_list().unwrap()).len();

        let quit = quit_desktop(&work, QUIT_TIMEOUT);

        let left = desktop_pids(&work, &process_list().unwrap()).len();
        // Closing their stdin ends the stand-ins whatever happened, so the
        // test never waits on them.
        drop((first_stdin, second_stdin));
        first.join().unwrap();
        second.join().unwrap();
        assert_eq!(running, 2);
        quit.unwrap();
        assert_eq!(left, 0);
    }

    #[test]
    fn quitting_a_desktop_app_that_isnt_running_does_nothing() {
        let root = tempdir().unwrap();

        quit_desktop(&claude_home(root.path(), "Work"), QUIT_TIMEOUT).unwrap();
    }

    #[test]
    fn a_desktop_app_that_wont_quit_is_given_up_on_at_the_timeout() {
        let root = tempdir().unwrap();
        let work = claude_home(root.path(), "Work");
        let mut stubborn = stubborn_wrapper_process(root.path(), &work.gui_data_dir);

        let quit = quit_desktop(&work, Duration::from_millis(600));

        let still_running = stubborn.try_wait().unwrap().is_none();
        stubborn.kill().unwrap();
        stubborn.wait().unwrap();
        assert!(
            matches!(&quit, Err(AppError::Validation(message)) if message == "Claude (Work) didn't quit"),
            "{quit:?}"
        );
        assert!(still_running);
    }
}

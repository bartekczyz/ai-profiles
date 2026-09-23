//! The desktop app instance a home's sessions can be open in.

use super::Home;
use crate::launch::{find_running_pid, first_pid_ending_with};

/// The pid of the main process of `home`'s desktop app instance, given the
/// output of `ps -ax -o pid=,command=`, if it runs.
///
/// A profile's instance runs with `--user-data-dir=<its gui data dir>`, and so
/// does the stock one when ai-profiles started it (see [`find_running_pid`]).
/// The stock app started from the Dock or Finder runs with no arguments at
/// all, which also makes it the stock home's instance.
pub fn desktop_pid(home: &Home, ps_output: &str) -> Option<i32> {
    let data_dir = home.gui_data_dir.display().to_string();
    home.app
        .spec()
        .gui_bundle_candidates
        .iter()
        .find_map(|candidate| {
            find_running_pid(ps_output, &data_dir, candidate.macos_exec).or_else(|| {
                let bare = format!("/Contents/MacOS/{}", candidate.macos_exec);
                home.stock
                    .then(|| first_pid_ending_with(ps_output, &[bare]))
                    .flatten()
            })
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::app_kind::AppKind;

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
    fn helpers_and_other_profiles_are_not_the_stock_instance() {
        let ps_output = format!(
            "  701 /Applications/ChatGPT.app/Contents/Frameworks/ChatGPT Helper.app/Contents/MacOS/ChatGPT Helper --type=renderer\n  \
             710 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT --user-data-dir={PROFILE_DIR}\n"
        );

        assert_eq!(desktop_pid(&home(STOCK_DIR, true), &ps_output), None);
    }
}

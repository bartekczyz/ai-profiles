// SPDX-License-Identifier: MIT

//! The command line, which exists for one caller: a wrapper bundle that found
//! itself out of step with the vendor app it was cloned from.
//!
//! A wrapper cannot rebuild itself. The rebuild deletes and recreates the
//! bundle, and the only process in a position to notice the drift is the shim
//! running out of that very bundle. So it hands the launch back here instead:
//! `ai-profiles --open-profile <id>` opens one profile through the usual route
//! — which rebuilds a stale wrapper on the way — and exits, without ever
//! starting Tauri or showing a window.

use crate::launch;
use crate::profiles;

/// Flag that opens one profile and exits, instead of starting the GUI. Defined
/// with the shim's keys, because the shim is what passes it.
pub use profile_shim::OPEN_PROFILE_FLAG;

/// What the process was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Invocation<'a> {
    /// Start the GUI, which is what every ordinary launch wants.
    Gui,
    /// Open this profile's desktop app, then exit.
    OpenProfile(&'a str),
    /// The arguments named a mode without giving it what it needs.
    Misuse(String),
}

/// Pure: what `args` — the whole argument vector, program name included — asks
/// for.
///
/// Anything the flag does not appear in is an ordinary launch: LaunchServices
/// passes arguments of its own (`-psn_0_…`), and an unknown one must never
/// stop the app from opening.
pub fn invocation(args: &[String]) -> Invocation<'_> {
    let Some(offset) = args.iter().skip(1).position(|arg| arg == OPEN_PROFILE_FLAG) else {
        return Invocation::Gui;
    };
    match args.get(offset + 2) {
        Some(id) if !id.trim().is_empty() => Invocation::OpenProfile(id),
        _ => Invocation::Misuse(format!("{OPEN_PROFILE_FLAG} needs a profile id")),
    }
}

/// Open the desktop app of the profile `id`, rebuilding its wrapper first if
/// that has fallen behind the vendor app.
///
/// Returns once the app is up, which can take a minute in the worst case: a
/// rebuild takes seconds, and macOS holds the first run of a bundle it has not
/// assessed before. The caller is a detached process nobody is waiting on, so
/// that is fine here.
pub fn open_profile(id: &str) -> Result<(), String> {
    let all = profiles::load().map_err(|err| err.to_string())?;
    let profile = all
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| format!("no profile with id {id}"))?;
    if !profile.surfaces.gui {
        return Err(format!("profile {id} has no desktop surface"));
    }

    let bypass = launch::open_profile(profile, env!("CARGO_PKG_VERSION"), launch::focus_pid)
        .map_err(|err| err.to_string())?;
    // Not a failure: the profile is open, just not through its wrapper. Worth
    // saying, because this is the only trace such a launch leaves anywhere.
    if let Some(bypass) = bypass {
        eprintln!("ai-profiles: {bypass}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        std::iter::once("/Applications/ai-profiles.app/Contents/MacOS/ai-profiles")
            .chain(values.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn no_arguments_starts_the_gui() {
        assert_eq!(invocation(&args(&[])), Invocation::Gui);
    }

    #[test]
    fn arguments_macos_adds_of_its_own_start_the_gui() {
        assert_eq!(invocation(&args(&["-psn_0_1466781"])), Invocation::Gui);
        assert_eq!(invocation(&args(&["--some-day"])), Invocation::Gui);
    }

    #[test]
    fn the_flag_takes_the_argument_after_it_as_the_profile_id() {
        assert_eq!(
            invocation(&args(&[OPEN_PROFILE_FLAG, "abc-123"])),
            Invocation::OpenProfile("abc-123")
        );
    }

    #[test]
    fn the_flag_is_found_wherever_it_sits() {
        assert_eq!(
            invocation(&args(&["-psn_0_1466781", OPEN_PROFILE_FLAG, "abc-123"])),
            Invocation::OpenProfile("abc-123")
        );
    }

    #[test]
    fn the_flag_without_a_usable_id_is_a_misuse_rather_than_a_silent_gui_launch() {
        for tail in [vec![OPEN_PROFILE_FLAG], vec![OPEN_PROFILE_FLAG, "   "]] {
            let argv = args(&tail);
            let parsed = invocation(&argv);
            assert!(
                matches!(parsed, Invocation::Misuse(_)),
                "{tail:?} parsed as {parsed:?}"
            );
        }
    }

    #[test]
    fn a_program_path_that_looks_like_the_flag_is_not_the_flag() {
        let only_argv0 = vec![OPEN_PROFILE_FLAG.to_owned()];
        assert_eq!(invocation(&only_argv0), Invocation::Gui);
    }
}

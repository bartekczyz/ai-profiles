use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

use profile_shim::{
    build_argv, built_from_version, bundle_version, handoff, info_plist_path, launch_params,
    rebuild_argv, should_hand_off, vendor_binary_path, Handoff, PROBE_ENV,
};

fn main() -> ExitCode {
    if std::env::var_os(PROBE_ENV).is_some() {
        return ExitCode::SUCCESS;
    }
    match run() {
        Ok(Outcome::HandedOff) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("profile-shim: {message}");
            ExitCode::FAILURE
        }
    }
}

/// What the shim did instead of becoming the vendor app. `exec` never returns,
/// so handing the launch to ai-profiles is the only way to finish successfully
/// and still have a process here to say so.
enum Outcome {
    HandedOff,
}

fn run() -> Result<Outcome, String> {
    let shim =
        std::env::current_exe().map_err(|err| format!("cannot resolve its own path: {err}"))?;
    let info_path = info_plist_path(&shim)
        .ok_or_else(|| format!("{} is not inside a bundle", shim.display()))?;
    let info = plist::Value::from_file(&info_path)
        .map_err(|err| format!("cannot read {}: {err}", info_path.display()))?;
    let dictionary = info
        .as_dictionary()
        .ok_or_else(|| format!("{} is not a dictionary", info_path.display()))?;
    let params =
        launch_params(dictionary).map_err(|err| format!("{}: {err}", info_path.display()))?;

    if let Some(handoff) = handoff(dictionary) {
        if should_hand_off(
            built_from_version(dictionary).as_deref(),
            bundle_version(&handoff.vendor_bundle).as_deref(),
        ) && hand_off(&handoff)
        {
            return Ok(Outcome::HandedOff);
        }
    }

    let vendor = vendor_binary_path(&shim);
    let mut command = Command::new(&vendor);
    if let Some((name, value)) = &params.config_env {
        command.env(name, value);
    }
    command.args(build_argv(&params, std::env::args_os().skip(1)));

    let err = command.exec();
    Err(format!("cannot exec {}: {err}", vendor.display()))
}

/// Ask ai-profiles to rebuild this wrapper and open the profile again, and say
/// whether that request got away.
///
/// The child outlives this process on purpose: the rebuild deletes and recreates
/// the bundle we are running from, so the only safe thing to do afterwards is
/// to leave. It is never waited on.
///
/// A request that cannot be started is not fatal. An app running the version it
/// was cloned from beats no app at all, so the caller falls through and starts
/// the vendor binary it already has.
fn hand_off(handoff: &Handoff) -> bool {
    match Command::new(&handoff.host_binary)
        .args(rebuild_argv(&handoff.profile_id))
        .spawn()
    {
        Ok(_child) => true,
        Err(err) => {
            eprintln!(
                "profile-shim: cannot ask {} for a rebuild: {err}",
                handoff.host_binary.display()
            );
            false
        }
    }
}

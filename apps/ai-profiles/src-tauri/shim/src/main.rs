use std::convert::Infallible;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

use profile_shim::{build_argv, info_plist_path, launch_params, vendor_binary_path, PROBE_ENV};

fn main() -> ExitCode {
    if std::env::var_os(PROBE_ENV).is_some() {
        return ExitCode::SUCCESS;
    }
    match run() {
        Ok(never) => match never {},
        Err(message) => {
            eprintln!("profile-shim: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Only ever returns on failure: success replaces this process via `exec`.
fn run() -> Result<Infallible, String> {
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

    let vendor = vendor_binary_path(&shim);
    let mut command = Command::new(&vendor);
    if let Some((name, value)) = &params.config_env {
        command.env(name, value);
    }
    command.args(build_argv(&params, std::env::args_os().skip(1)));

    let err = command.exec();
    Err(format!("cannot exec {}: {err}", vendor.display()))
}

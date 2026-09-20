//! The `profile-shim` binary, embedded in the app.
//!
//! build.rs compiles the `shim/` crate for the architecture this build targets,
//! and a wrapper bundle gets a copy of it as its executable. What the shim does
//! at launch is documented on the `profile-shim` crate.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::error::AppResult;

/// The shim, ready to be written into a wrapper's `Contents/MacOS`.
static PROFILE_SHIM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/profile-shim"));

/// Write the shim to `destination` as an executable file.
pub fn install(destination: &Path) -> AppResult<()> {
    fs::write(destination, PROFILE_SHIM)?;
    let mut permissions = fs::metadata(destination)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(destination, permissions)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::Command;

    use plist::{Dictionary, Value};
    use profile_shim::{CONFIG_ENV_NAME_KEY, CONFIG_ENV_VALUE_KEY, USER_DATA_DIR_KEY};

    use super::*;

    /// Mach-O 64-bit magic as laid out on disk: arm64 and x86_64 both use it.
    const MACH_O_64_MAGIC: [u8; 4] = [0xcf, 0xfa, 0xed, 0xfe];

    fn info(entries: &[(&str, &str)]) -> Dictionary {
        let mut dictionary = Dictionary::new();
        for (key, value) in entries {
            dictionary.insert((*key).to_owned(), Value::String((*value).to_owned()));
        }
        dictionary
    }

    /// Lay out `<root>/Fake.app` the way a wrapper is built: the shim as the
    /// executable, a stand-in vendor binary beside it as `Fake.bin`, and `info`
    /// as `Info.plist`. The stand-in writes the arguments it received, and the
    /// value of `AI_PROFILES_TEST_HOME`, to `record`. Returns the shim's path.
    fn fake_wrapper(root: &Path, info: Dictionary, record: &Path) -> PathBuf {
        let macos = root.join("Fake.app/Contents/MacOS");
        fs::create_dir_all(&macos).unwrap();
        Value::Dictionary(info)
            .to_file_xml(root.join("Fake.app/Contents/Info.plist"))
            .unwrap();

        let shim = macos.join("Fake");
        install(&shim).unwrap();

        let vendor = macos.join("Fake.bin");
        fs::write(
            &vendor,
            format!(
                "#!/bin/sh\n{{\n  for argument in \"$@\"; do printf 'arg=%s\\n' \"$argument\"; done\n  \
                 printf 'env=%s\\n' \"${{AI_PROFILES_TEST_HOME-unset}}\"\n}} > '{}'\n",
                record.display()
            ),
        )
        .unwrap();
        let mut permissions = fs::metadata(&vendor).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&vendor, permissions).unwrap();
        shim
    }

    fn run_shim(shim: &Path, arguments: &[&str]) -> bool {
        Command::new(shim)
            .args(arguments)
            .env_remove("AI_PROFILES_TEST_HOME")
            .status()
            .unwrap()
            .success()
    }

    #[test]
    fn embedded_shim_is_a_64_bit_mach_o() {
        assert_eq!(PROFILE_SHIM[..4], MACH_O_64_MAGIC);
    }

    #[test]
    fn install_writes_the_shim_as_an_executable() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("shim");

        install(&destination).unwrap();

        assert_eq!(fs::read(&destination).unwrap(), PROFILE_SHIM);
        let mode = fs::metadata(&destination).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn shim_execs_the_vendor_binary_with_the_injected_flag_and_env() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("record");
        let shim = fake_wrapper(
            dir.path(),
            info(&[
                (USER_DATA_DIR_KEY, "/tmp/profile/gui data"),
                (CONFIG_ENV_NAME_KEY, "AI_PROFILES_TEST_HOME"),
                (CONFIG_ENV_VALUE_KEY, "/tmp/profile/cli-config"),
            ]),
            &record,
        );

        assert!(run_shim(&shim, &["--flag", "two words"]));

        assert_eq!(
            fs::read_to_string(&record).unwrap(),
            "arg=--user-data-dir=/tmp/profile/gui data\n\
             arg=--flag\n\
             arg=two words\n\
             env=/tmp/profile/cli-config\n"
        );
    }

    #[test]
    fn shim_sets_no_env_var_when_none_is_configured() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("record");
        let shim = fake_wrapper(
            dir.path(),
            info(&[(USER_DATA_DIR_KEY, "/tmp/profile/gui-data")]),
            &record,
        );

        assert!(run_shim(&shim, &[]));

        assert_eq!(
            fs::read_to_string(&record).unwrap(),
            "arg=--user-data-dir=/tmp/profile/gui-data\nenv=unset\n"
        );
    }

    #[test]
    fn shim_started_only_to_warm_up_exits_without_starting_the_vendor_binary() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("record");
        let shim = fake_wrapper(
            dir.path(),
            info(&[(USER_DATA_DIR_KEY, "/tmp/profile/gui-data")]),
            &record,
        );

        let status = Command::new(&shim)
            .env(profile_shim::PROBE_ENV, "1")
            .status()
            .unwrap();

        assert!(status.success());
        assert!(!record.exists(), "vendor binary must not run");
    }

    #[test]
    fn shim_started_only_to_warm_up_does_not_need_a_configured_bundle() {
        // What it is asked to do is nothing, so a bundle it could not launch is
        // as good as any to run it in.
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("record");
        let shim = fake_wrapper(dir.path(), info(&[]), &record);

        let status = Command::new(&shim)
            .env(profile_shim::PROBE_ENV, "1")
            .status()
            .unwrap();

        assert!(status.success());
    }

    #[test]
    fn shim_refuses_to_start_without_a_user_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("record");
        let shim = fake_wrapper(dir.path(), info(&[]), &record);

        assert!(!run_shim(&shim, &[]));
        assert!(!record.exists(), "vendor binary must not run");
    }

    #[test]
    fn shim_fails_when_the_vendor_binary_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("record");
        let shim = fake_wrapper(
            dir.path(),
            info(&[(USER_DATA_DIR_KEY, "/tmp/profile/gui-data")]),
            &record,
        );
        fs::remove_file(shim.with_file_name("Fake.bin")).unwrap();

        assert!(!run_shim(&shim, &[]));
    }
}

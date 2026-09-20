//! Logic behind `profile-shim`, the executable of a per-profile wrapper bundle.
//!
//! A wrapper is a copy of a vendor `.app` whose own executable was renamed to
//! `<exec>.bin`, with this shim installed under the original name. macOS takes
//! a running app's identity (Dock icon, label, Cmd-Tab entry) from the bundle
//! its executable lives in, and a Dock click launches that bundle with no
//! arguments. So the shim re-adds what the stock launcher would have passed and
//! then execs the vendor binary *inside the wrapper*, which keeps the process
//! attached to the wrapper's identity.
//!
//! The parameters come from the wrapper's own `Info.plist`, so a single
//! prebuilt shim serves every profile.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use plist::{Dictionary, Value};

/// Env var that makes the shim exit successfully without doing anything.
///
/// macOS holds the first execution of a bundle it has not seen before for
/// several seconds while it assesses it (about five on the machine this was
/// measured on, whichever way the bundle is started). Running a new wrapper
/// once this way, right after building it, has that happen then instead of on
/// the first launch, when it would look like the app failing to start.
pub const PROBE_ENV: &str = "AI_PROFILES_SHIM_PROBE";

/// `Info.plist` key holding the value of `--user-data-dir`. Required.
pub const USER_DATA_DIR_KEY: &str = "AIProfilesUserDataDir";

/// `Info.plist` key naming an env var to set before exec (e.g. `CODEX_HOME`).
/// Optional, but must be paired with [`CONFIG_ENV_VALUE_KEY`].
pub const CONFIG_ENV_NAME_KEY: &str = "AIProfilesConfigEnvName";

/// `Info.plist` key holding the value for [`CONFIG_ENV_NAME_KEY`].
pub const CONFIG_ENV_VALUE_KEY: &str = "AIProfilesConfigEnvValue";

/// Suffix appended to the shim's file name to get the vendor binary it execs.
pub const VENDOR_BINARY_SUFFIX: &str = ".bin";

/// What the shim needs to know to launch one profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchParams {
    /// Value passed to the vendor binary as `--user-data-dir=<value>`.
    pub user_data_dir: String,
    /// `(name, value)` env var set before exec, for apps that read their
    /// account from an env var rather than from `--user-data-dir`.
    pub config_env: Option<(String, String)>,
}

/// Why an `Info.plist` did not yield usable [`LaunchParams`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamsError {
    /// [`USER_DATA_DIR_KEY`] is absent, empty or not a string.
    MissingUserDataDir,
    /// Only one of the config env keys is set, or the pair is not a valid
    /// env var (empty, or a name containing `=`).
    InvalidConfigEnv,
}

impl fmt::Display for ParamsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParamsError::MissingUserDataDir => {
                write!(formatter, "{USER_DATA_DIR_KEY} is missing or empty")
            }
            ParamsError::InvalidConfigEnv => write!(
                formatter,
                "{CONFIG_ENV_NAME_KEY} and {CONFIG_ENV_VALUE_KEY} must be set together \
                 to a valid env var"
            ),
        }
    }
}

impl std::error::Error for ParamsError {}

/// Read the launch parameters out of a wrapper's `Info.plist`. Fails closed: a
/// half-configured wrapper must not start the vendor app on some other data
/// directory.
pub fn launch_params(info: &Dictionary) -> Result<LaunchParams, ParamsError> {
    let user_data_dir =
        non_empty_string(info.get(USER_DATA_DIR_KEY)).ok_or(ParamsError::MissingUserDataDir)?;

    let name = info.get(CONFIG_ENV_NAME_KEY);
    let value = info.get(CONFIG_ENV_VALUE_KEY);
    let config_env = if name.is_none() && value.is_none() {
        None
    } else {
        let name = non_empty_string(name)
            .filter(|name| !name.contains('='))
            .ok_or(ParamsError::InvalidConfigEnv)?;
        let value = non_empty_string(value).ok_or(ParamsError::InvalidConfigEnv)?;
        Some((name, value))
    };

    Ok(LaunchParams {
        user_data_dir,
        config_env,
    })
}

/// The value as an owned string, if it is a plist string and not empty.
fn non_empty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_string)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Arguments for the vendor binary: the injected `--user-data-dir` first, then
/// whatever the shim itself was given, untouched and in order.
pub fn build_argv(
    params: &LaunchParams,
    passthrough: impl IntoIterator<Item = OsString>,
) -> Vec<OsString> {
    let mut argv = vec![OsString::from(format!(
        "--user-data-dir={}",
        params.user_data_dir
    ))];
    argv.extend(passthrough);
    argv
}

/// The vendor binary next to the shim: `<exec>` becomes `<exec>.bin`.
pub fn vendor_binary_path(shim: &Path) -> PathBuf {
    let mut file_name = shim.file_name().unwrap_or_default().to_os_string();
    file_name.push(VENDOR_BINARY_SUFFIX);
    shim.with_file_name(file_name)
}

/// The `Info.plist` of the bundle a shim at `<bundle>/Contents/MacOS/<exec>`
/// lives in, or `None` if the path is too shallow to be inside a bundle.
pub fn info_plist_path(shim: &Path) -> Option<PathBuf> {
    let contents = shim.parent()?.parent()?;
    Some(contents.join("Info.plist"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(entries: &[(&str, &str)]) -> Dictionary {
        let mut dictionary = Dictionary::new();
        for (key, value) in entries {
            dictionary.insert((*key).to_owned(), Value::String((*value).to_owned()));
        }
        dictionary
    }

    fn params(user_data_dir: &str) -> LaunchParams {
        LaunchParams {
            user_data_dir: user_data_dir.to_owned(),
            config_env: None,
        }
    }

    fn os_strings(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn argv_is_just_the_injected_flag_when_nothing_was_passed() {
        assert_eq!(
            build_argv(&params("/data"), Vec::new()),
            os_strings(&["--user-data-dir=/data"])
        );
    }

    #[test]
    fn argv_keeps_passthrough_arguments_in_order_after_the_flag() {
        assert_eq!(
            build_argv(
                &params("/data"),
                os_strings(&["--first", "second", "--third"])
            ),
            os_strings(&["--user-data-dir=/data", "--first", "second", "--third"])
        );
    }

    #[test]
    fn argv_keeps_a_data_dir_with_spaces_as_a_single_argument() {
        let argv = build_argv(&params("/Application Support/gui data"), Vec::new());
        assert_eq!(argv.len(), 1);
        assert_eq!(
            argv[0],
            OsString::from("--user-data-dir=/Application Support/gui data")
        );
    }

    #[test]
    fn params_require_the_user_data_dir() {
        assert_eq!(
            launch_params(&info(&[])),
            Err(ParamsError::MissingUserDataDir)
        );
        assert_eq!(
            launch_params(&info(&[(USER_DATA_DIR_KEY, "")])),
            Err(ParamsError::MissingUserDataDir)
        );
    }

    #[test]
    fn params_reject_a_user_data_dir_that_is_not_a_string() {
        let mut dictionary = Dictionary::new();
        dictionary.insert(USER_DATA_DIR_KEY.to_owned(), Value::Integer(7.into()));
        assert_eq!(
            launch_params(&dictionary),
            Err(ParamsError::MissingUserDataDir)
        );
    }

    #[test]
    fn params_without_a_config_env_pair_have_none() {
        assert_eq!(
            launch_params(&info(&[
                (USER_DATA_DIR_KEY, "/data"),
                ("CFBundleName", "Claude")
            ])),
            Ok(params("/data"))
        );
    }

    #[test]
    fn params_read_the_config_env_pair() {
        assert_eq!(
            launch_params(&info(&[
                (USER_DATA_DIR_KEY, "/data"),
                (CONFIG_ENV_NAME_KEY, "CODEX_HOME"),
                (CONFIG_ENV_VALUE_KEY, "/cfg"),
            ])),
            Ok(LaunchParams {
                user_data_dir: "/data".to_owned(),
                config_env: Some(("CODEX_HOME".to_owned(), "/cfg".to_owned())),
            })
        );
    }

    #[test]
    fn params_reject_a_config_env_with_only_one_half() {
        assert_eq!(
            launch_params(&info(&[
                (USER_DATA_DIR_KEY, "/data"),
                (CONFIG_ENV_NAME_KEY, "CODEX_HOME"),
            ])),
            Err(ParamsError::InvalidConfigEnv)
        );
        assert_eq!(
            launch_params(&info(&[
                (USER_DATA_DIR_KEY, "/data"),
                (CONFIG_ENV_VALUE_KEY, "/cfg"),
            ])),
            Err(ParamsError::InvalidConfigEnv)
        );
    }

    #[test]
    fn params_reject_an_env_name_that_cannot_be_set() {
        assert_eq!(
            launch_params(&info(&[
                (USER_DATA_DIR_KEY, "/data"),
                (CONFIG_ENV_NAME_KEY, "A=B"),
                (CONFIG_ENV_VALUE_KEY, "/cfg"),
            ])),
            Err(ParamsError::InvalidConfigEnv)
        );
        assert_eq!(
            launch_params(&info(&[
                (USER_DATA_DIR_KEY, "/data"),
                (CONFIG_ENV_NAME_KEY, ""),
                (CONFIG_ENV_VALUE_KEY, "/cfg"),
            ])),
            Err(ParamsError::InvalidConfigEnv)
        );
    }

    #[test]
    fn vendor_binary_is_the_shim_name_plus_bin() {
        assert_eq!(
            vendor_binary_path(Path::new(
                "/Applications/Claude (Work).app/Contents/MacOS/Claude"
            )),
            PathBuf::from("/Applications/Claude (Work).app/Contents/MacOS/Claude.bin")
        );
    }

    #[test]
    fn info_plist_sits_in_contents_above_the_macos_dir() {
        assert_eq!(
            info_plist_path(Path::new("/Applications/X.app/Contents/MacOS/X")),
            Some(PathBuf::from("/Applications/X.app/Contents/Info.plist"))
        );
    }

    #[test]
    fn info_plist_is_none_outside_a_bundle_layout() {
        assert_eq!(info_plist_path(Path::new("shim")), None);
        assert_eq!(info_plist_path(Path::new("/shim")), None);
    }
}

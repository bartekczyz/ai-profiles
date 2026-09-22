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

/// `Info.plist` key holding the version of the vendor app the wrapper was
/// cloned from, so the shim can tell whether the vendor has moved on without
/// it. Written by the build; see the app's `wrapper::info_plist`.
pub const VENDOR_VERSION_KEY: &str = "AIProfilesVendorVersion";

/// `Info.plist` key holding the id of the profile the wrapper belongs to: the
/// argument the host binary is given to open it.
///
/// One of the three handoff keys ([`VENDOR_BUNDLE_KEY`], [`HOST_BINARY_KEY`]),
/// which are written together. A wrapper built before they existed has none of
/// them and simply starts the version it has.
pub const PROFILE_ID_KEY: &str = "AIProfilesProfileId";

/// `Info.plist` key holding the path of the vendor `.app` the wrapper was
/// cloned from, to read the installed version out of at launch.
pub const VENDOR_BUNDLE_KEY: &str = "AIProfilesVendorBundle";

/// `Info.plist` key holding the path of the ai-profiles executable to hand a
/// launch back to when the wrapper has fallen behind the vendor app.
pub const HOST_BINARY_KEY: &str = "AIProfilesHostBinary";

/// Suffix appended to the shim's file name to get the vendor binary it execs.
pub const VENDOR_BINARY_SUFFIX: &str = ".bin";

/// ai-profiles' flag for opening one profile and exiting, which is what a
/// wrapper asks for when it finds itself behind the vendor app.
pub const OPEN_PROFILE_FLAG: &str = "--open-profile";

/// What the shim needs to have its own wrapper rebuilt.
///
/// A wrapper is a clone of one version of the vendor app, and the vendor's own
/// updater cannot install into it: the clone carries a different bundle id and
/// an ad-hoc signature, so the update is rejected. Left alone it would go on
/// running the version it was cloned from for good. Instead it hands the launch
/// back to ai-profiles, which rebuilds it from the installed vendor app and
/// opens it again — the rebuild replaces the very bundle the shim runs from, so
/// it cannot be done here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handoff {
    /// The profile to open: [`OPEN_PROFILE_FLAG`]'s argument.
    pub profile_id: String,
    /// The vendor `.app` this wrapper was cloned from, to read the installed
    /// version out of.
    pub vendor_bundle: PathBuf,
    /// The ai-profiles executable to run.
    pub host_binary: PathBuf,
}

/// The handoff a wrapper's `Info.plist` records, or `None` when it records no
/// usable one.
///
/// All or nothing: the three keys are written together, and a wrapper built
/// before they existed has none of them. Without a complete set the shim simply
/// starts the version it has, which is what it did before any of this.
pub fn handoff(info: &Dictionary) -> Option<Handoff> {
    Some(Handoff {
        profile_id: non_empty_string(info.get(PROFILE_ID_KEY))?,
        vendor_bundle: PathBuf::from(non_empty_string(info.get(VENDOR_BUNDLE_KEY))?),
        host_binary: PathBuf::from(non_empty_string(info.get(HOST_BINARY_KEY))?),
    })
}

/// The vendor version a wrapper's `Info.plist` says it was cloned from.
pub fn built_from_version(info: &Dictionary) -> Option<String> {
    non_empty_string(info.get(VENDOR_VERSION_KEY))
}

/// The `CFBundleVersion` of the bundle at `bundle`, or `None` if it cannot be
/// read. Read off the vendor app, to compare with [`built_from_version`].
pub fn bundle_version(bundle: &Path) -> Option<String> {
    let info = Value::from_file(bundle.join("Contents/Info.plist")).ok()?;
    non_empty_string(info.as_dictionary()?.get("CFBundleVersion"))
}

/// Pure: whether a wrapper cloned from `built_from` should hand its launch
/// back, given the vendor app now reporting `vendor_now`.
///
/// Any difference counts — a downgrade or a replacement as much as an update —
/// and so does a wrapper that does not say what it was cloned from, since there
/// is no telling.
///
/// A vendor version that cannot be read is no reason to hand anything off:
/// there is nothing to compare with, and a rebuild against an app that cannot
/// be read would fail anyway. Starting the version the wrapper has is the
/// better answer.
pub fn should_hand_off(built_from: Option<&str>, vendor_now: Option<&str>) -> bool {
    match vendor_now {
        None => false,
        Some(current) => built_from != Some(current),
    }
}

/// The arguments that ask ai-profiles to open `profile_id`, rebuilding its
/// wrapper on the way.
pub fn rebuild_argv(profile_id: &str) -> Vec<OsString> {
    vec![
        OsString::from(OPEN_PROFILE_FLAG),
        OsString::from(profile_id),
    ]
}

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

    /// The three handoff keys, complete.
    fn handoff_entries() -> Vec<(&'static str, &'static str)> {
        vec![
            (PROFILE_ID_KEY, "profile-1"),
            (VENDOR_BUNDLE_KEY, "/Applications/Claude.app"),
            (
                HOST_BINARY_KEY,
                "/Applications/ai-profiles.app/Contents/MacOS/ai-profiles",
            ),
        ]
    }

    #[test]
    fn a_complete_set_of_keys_is_a_handoff() {
        assert_eq!(
            handoff(&info(&handoff_entries())),
            Some(Handoff {
                profile_id: "profile-1".to_owned(),
                vendor_bundle: PathBuf::from("/Applications/Claude.app"),
                host_binary: PathBuf::from(
                    "/Applications/ai-profiles.app/Contents/MacOS/ai-profiles"
                ),
            })
        );
    }

    #[test]
    fn a_wrapper_built_before_the_handoff_existed_records_none() {
        assert_eq!(handoff(&info(&[(USER_DATA_DIR_KEY, "/data")])), None);
    }

    #[test]
    fn one_key_short_or_empty_is_no_handoff_at_all() {
        for dropped in [PROFILE_ID_KEY, VENDOR_BUNDLE_KEY, HOST_BINARY_KEY] {
            let partial: Vec<_> = handoff_entries()
                .into_iter()
                .filter(|(key, _)| *key != dropped)
                .collect();
            assert_eq!(handoff(&info(&partial)), None, "without {dropped}");

            let emptied: Vec<_> = handoff_entries()
                .into_iter()
                .map(|(key, value)| {
                    if key == dropped {
                        (key, "")
                    } else {
                        (key, value)
                    }
                })
                .collect();
            assert_eq!(handoff(&info(&emptied)), None, "{dropped} empty");
        }
    }

    #[test]
    fn a_wrapper_hands_off_only_when_the_vendor_reports_another_version() {
        assert!(!should_hand_off(Some("2.0"), Some("2.0")), "equal");
        assert!(should_hand_off(Some("2.0"), Some("2.1")), "vendor newer");
        assert!(should_hand_off(Some("2.1"), Some("2.0")), "vendor older");
        assert!(should_hand_off(None, Some("2.0")), "nothing recorded");
    }

    #[test]
    fn a_vendor_version_that_cannot_be_read_starts_the_version_at_hand() {
        assert!(!should_hand_off(Some("2.0"), None));
        assert!(!should_hand_off(None, None));
    }

    #[test]
    fn the_recorded_vendor_version_is_read_back_when_it_is_usable() {
        assert_eq!(
            built_from_version(&info(&[(VENDOR_VERSION_KEY, "2.2553.13")])),
            Some("2.2553.13".to_owned())
        );
        assert_eq!(built_from_version(&info(&[(VENDOR_VERSION_KEY, "")])), None);
        assert_eq!(built_from_version(&info(&[])), None);
    }

    #[test]
    fn a_rebuild_is_asked_for_by_profile_id() {
        assert_eq!(
            rebuild_argv("profile-1"),
            vec![
                OsString::from("--open-profile"),
                OsString::from("profile-1")
            ]
        );
    }
}

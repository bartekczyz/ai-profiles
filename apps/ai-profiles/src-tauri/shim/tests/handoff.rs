//! What the shim binary actually does when it is started, which is the only
//! place the handoff decision and the exec meet.
//!
//! Each case builds a throwaway wrapper — an `Info.plist`, the real shim as its
//! executable, and a script standing in for the vendor binary — then runs the
//! shim and looks at which script got to run.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use profile_shim::{
    HOST_BINARY_KEY, PROFILE_ID_KEY, USER_DATA_DIR_KEY, VENDOR_BUNDLE_KEY, VENDOR_VERSION_KEY,
};

const SHIM: &str = env!("CARGO_BIN_EXE_profile-shim");

/// How long a script spawned by the shim is given to leave its mark. Generous:
/// the point is never to wait this long, only not to race.
const SETTLE: Duration = Duration::from_secs(10);

/// A script at `path` that writes its arguments to `<path>.ran`, one per line.
fn recording_script(path: &Path) {
    let record = format!("{}.ran", path.display());
    fs::write(
        path,
        format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{record}'\n"),
    )
    .unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// The lines a [`recording_script`] recorded, or `None` if it never ran.
fn ran(path: &Path) -> Option<Vec<String>> {
    let record = PathBuf::from(format!("{}.ran", path.display()));
    let text = fs::read_to_string(record).ok()?;
    Some(text.lines().map(str::to_owned).collect())
}

/// Wait until `path` has run, or give up. Returns what it recorded.
fn wait_for(path: &Path) -> Option<Vec<String>> {
    let deadline = Instant::now() + SETTLE;
    while Instant::now() < deadline {
        if let Some(lines) = ran(path) {
            return Some(lines);
        }
        thread::sleep(Duration::from_millis(25));
    }
    None
}

/// A bundle at `<dir>/<name>.app` whose `Info.plist` holds `entries`.
fn bundle(dir: &Path, name: &str, entries: &[(&str, &str)]) -> PathBuf {
    let bundle = dir.join(format!("{name}.app"));
    fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
    let mut info = plist::Dictionary::new();
    for (key, value) in entries {
        info.insert((*key).to_owned(), plist::Value::String((*value).to_owned()));
    }
    plist::Value::Dictionary(info)
        .to_file_xml(bundle.join("Contents/Info.plist"))
        .unwrap();
    bundle
}

/// One wrapper ready to run: the shim as `Contents/MacOS/App`, a recording
/// script as the vendor binary beside it.
struct Wrapper {
    shim: PathBuf,
    vendor_binary: PathBuf,
}

fn wrapper(dir: &Path, entries: &[(&str, &str)]) -> Wrapper {
    let bundle = bundle(dir, "Wrapper", entries);
    let shim = bundle.join("Contents/MacOS/App");
    fs::copy(SHIM, &shim).unwrap();
    let vendor_binary = bundle.join("Contents/MacOS/App.bin");
    recording_script(&vendor_binary);
    Wrapper {
        shim,
        vendor_binary,
    }
}

/// A vendor app at version `version`, for the wrapper to compare itself with.
fn vendor(dir: &Path, version: &str) -> String {
    bundle(dir, "Vendor", &[("CFBundleVersion", version)])
        .display()
        .to_string()
}

/// A stand-in for the ai-profiles binary, which records how it was called.
fn host(dir: &Path) -> PathBuf {
    let host = dir.join("ai-profiles");
    recording_script(&host);
    host
}

fn run(shim: &Path) {
    let status = Command::new(shim).status().unwrap();
    assert!(status.success(), "the shim failed: {status}");
}

#[test]
fn a_wrapper_in_step_with_the_vendor_starts_the_vendor_binary() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path());
    let wrapper = wrapper(
        dir.path(),
        &[
            (USER_DATA_DIR_KEY, "/data/gui-data"),
            (VENDOR_VERSION_KEY, "2.0"),
            (PROFILE_ID_KEY, "profile-1"),
            (VENDOR_BUNDLE_KEY, &vendor(dir.path(), "2.0")),
            (HOST_BINARY_KEY, &host.display().to_string()),
        ],
    );

    run(&wrapper.shim);

    assert_eq!(
        wait_for(&wrapper.vendor_binary),
        Some(vec!["--user-data-dir=/data/gui-data".to_owned()])
    );
    assert_eq!(ran(&host), None, "nothing should have been handed off");
}

#[test]
fn a_wrapper_the_vendor_has_moved_on_from_asks_for_a_rebuild_instead() {
    let dir = tempfile::tempdir().unwrap();
    let host = host(dir.path());
    let wrapper = wrapper(
        dir.path(),
        &[
            (USER_DATA_DIR_KEY, "/data/gui-data"),
            (VENDOR_VERSION_KEY, "2.0"),
            (PROFILE_ID_KEY, "profile-1"),
            (VENDOR_BUNDLE_KEY, &vendor(dir.path(), "2.1")),
            (HOST_BINARY_KEY, &host.display().to_string()),
        ],
    );

    run(&wrapper.shim);

    assert_eq!(
        wait_for(&host),
        Some(vec!["--open-profile".to_owned(), "profile-1".to_owned()])
    );
    assert_eq!(
        ran(&wrapper.vendor_binary),
        None,
        "the old version should not have been started as well"
    );
}

#[test]
fn a_wrapper_built_before_the_handoff_existed_starts_the_version_it_has() {
    let dir = tempfile::tempdir().unwrap();
    // Behind the vendor, but with no way to say so: exactly the shape of every
    // wrapper on disk before this existed.
    let wrapper = wrapper(
        dir.path(),
        &[
            (USER_DATA_DIR_KEY, "/data/gui-data"),
            (VENDOR_VERSION_KEY, "2.0"),
        ],
    );

    run(&wrapper.shim);

    assert_eq!(
        wait_for(&wrapper.vendor_binary),
        Some(vec!["--user-data-dir=/data/gui-data".to_owned()])
    );
}

#[test]
fn a_handoff_that_cannot_be_started_still_opens_the_app() {
    let dir = tempfile::tempdir().unwrap();
    let wrapper = wrapper(
        dir.path(),
        &[
            (USER_DATA_DIR_KEY, "/data/gui-data"),
            (VENDOR_VERSION_KEY, "2.0"),
            (PROFILE_ID_KEY, "profile-1"),
            (VENDOR_BUNDLE_KEY, &vendor(dir.path(), "2.1")),
            // ai-profiles has been moved or deleted since the wrapper was built.
            (
                HOST_BINARY_KEY,
                &dir.path().join("gone").display().to_string(),
            ),
        ],
    );

    run(&wrapper.shim);

    assert_eq!(
        wait_for(&wrapper.vendor_binary),
        Some(vec!["--user-data-dir=/data/gui-data".to_owned()]),
        "an old app beats no app"
    );
}

#[test]
fn arguments_the_shim_was_given_reach_the_vendor_binary_after_the_injected_one() {
    let dir = tempfile::tempdir().unwrap();
    let wrapper = wrapper(
        dir.path(),
        &[
            (USER_DATA_DIR_KEY, "/data/gui-data"),
            (VENDOR_VERSION_KEY, "2.0"),
        ],
    );

    let status = Command::new(&wrapper.shim)
        .args(["--from-the-dock", "value"])
        .status()
        .unwrap();
    assert!(status.success());

    assert_eq!(
        wait_for(&wrapper.vendor_binary),
        Some(vec![
            "--user-data-dir=/data/gui-data".to_owned(),
            "--from-the-dock".to_owned(),
            "value".to_owned(),
        ])
    );
}

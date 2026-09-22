//! Per-profile wrapper bundles.
//!
//! Several profiles of one app show identical Dock tiles because they all run
//! out of the vendor's bundle, and macOS takes a running app's identity (Dock
//! icon and label, Cmd-Tab entry, menu-bar name) from the bundle its executable
//! lives in. A wrapper is a clone of the vendor app that *becomes* the running
//! app: it has its own bundle id, name and icon, and its executable is the
//! profile shim, which starts the vendor binary (moved to `<exec>.bin`, still
//! inside the wrapper) with the profile's `--user-data-dir`.
//!
//! Building one weakens the app's security: the clone is re-signed ad hoc, its
//! team-scoped entitlements are dropped (the kernel refuses to launch an ad-hoc
//! binary that claims them) and library validation is disabled (the vendor's
//! frameworks are signed by another team than the ad-hoc signature). A wrapper
//! therefore never leaves the machine that built it, and the vendor bundle is
//! only ever read.

mod entitlements;
mod info_plist;
mod sign;

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use plist::{Dictionary, Value};

use crate::error::{AppError, AppResult};
use crate::launchers::shim;

/// Name, without extension, of the icon the wrapper carries in
/// `Contents/Resources`.
const ICON_FILE: &str = "AppIcon";

/// Distinguishes the staging paths of builds running at the same time.
static NEXT_STAGING: AtomicUsize = AtomicUsize::new(0);

/// Everything one wrapper is built from.
pub struct WrapperRequest<'a> {
    /// The stock app to clone, e.g. `/Applications/Claude.app`.
    pub vendor_bundle: &'a Path,
    /// Where the wrapper is created. Must not exist, and must be on the same
    /// volume as `vendor_bundle` so the clone stays a clone.
    pub destination: &'a Path,
    /// `CFBundleIdentifier`, unique per profile.
    pub identifier: &'a str,
    /// `CFBundleDisplayName`, the Dock label.
    pub display_name: &'a str,
    /// The `.icns` bytes of the wrapper's icon.
    pub icon: &'a [u8],
    /// The profile's `--user-data-dir`.
    pub user_data_dir: &'a Path,
    /// The profile's id, recorded so the shim can ask for this wrapper by name
    /// when it needs rebuilding.
    pub profile_id: &'a str,
    /// The ai-profiles executable the shim hands a launch back to, recorded
    /// for the same reason.
    pub host_binary: &'a Path,
    /// This app's own version, recorded so an upgrade that changes what a
    /// wrapper contains — the shim above all — reaches the ones on disk.
    pub built_by: &'a str,
    /// `(name, value)` env var set before the vendor binary starts, for apps
    /// that read their account from one rather than from `--user-data-dir`
    /// (Codex: `CODEX_HOME`).
    pub config_env: Option<(&'a str, &'a Path)>,
}

/// Build the wrapper described by `request` at its destination.
///
/// The bundle is assembled beside the destination under a hidden name, checked,
/// and only then renamed into place, so a failure at any point leaves nothing
/// at the destination and nothing behind. Once in place it is run through once,
/// to have macOS assess it now rather than on its first launch (see `warm_up`),
/// which adds a few seconds on a machine where that takes a while.
pub fn build(request: &WrapperRequest<'_>) -> AppResult<()> {
    if request.destination.exists() {
        return Err(AppError::Validation(format!(
            "{} already exists",
            request.destination.display()
        )));
    }

    let vendor_info = read_info_plist(request.vendor_bundle)?;
    let executable = vendor_info
        .get("CFBundleExecutable")
        .and_then(Value::as_string)
        .ok_or_else(|| {
            AppError::Validation("the vendor Info.plist has no CFBundleExecutable".into())
        })?
        .to_owned();
    let vendor_signature = sign::inspect(request.vendor_bundle)?;
    let team_id = vendor_signature
        .team_id
        .clone()
        .or_else(|| entitlements::claimed_team_id(&vendor_signature.entitlements));

    let staging = Staging::new(request.destination)?;
    clone_bundle(request.vendor_bundle, &staging.bundle, &executable)?;

    let contents = staging.bundle.join("Contents");
    let info = info_plist::patch(
        &vendor_info,
        &info_plist::Patch {
            identifier: request.identifier,
            display_name: request.display_name,
            icon_file: ICON_FILE,
            user_data_dir: utf8(request.user_data_dir)?,
            profile_id: request.profile_id,
            vendor_bundle: utf8(request.vendor_bundle)?,
            host_binary: utf8(request.host_binary)?,
            built_by: request.built_by,
            config_env: request
                .config_env
                .map(|(name, value)| Ok::<_, AppError>((name, utf8(value)?)))
                .transpose()?,
        },
    )?;
    write_plist(&contents.join("Info.plist"), info)?;

    let resources = contents.join("Resources");
    fs::create_dir_all(&resources)?;
    fs::write(resources.join(format!("{ICON_FILE}.icns")), request.icon)?;

    // The vendor's executable moves aside and the shim takes its name, which is
    // what `CFBundleExecutable` still points at.
    let shim_path = contents.join("MacOS").join(&executable);
    let vendor_binary = profile_shim::vendor_binary_path(&shim_path);
    fs::rename(&shim_path, &vendor_binary)?;
    shim::install(&shim_path)?;

    let stripped =
        entitlements::strip_team_entitlements(&vendor_signature.entitlements, team_id.as_deref());
    write_plist(&staging.entitlements, stripped)?;
    // Inside out: the vendor binary first, because the bundle's signature seals
    // its hash.
    sign::sign(&vendor_binary, &staging.entitlements)?;
    sign::sign(&staging.bundle, &staging.entitlements)?;
    validate(&staging.bundle, &vendor_binary, team_id.as_deref())?;

    staging.commit(request.destination)?;
    // At its final path, which is where macOS will assess it.
    warm_up(&request.destination.join("Contents/MacOS").join(&executable));
    Ok(())
}

/// How long a wrapper is given to run through its warm-up before that is given
/// up on. The point is to have macOS's first-run hold happen at build time, so
/// this is generous compared with the five seconds it has been seen to take, and
/// it costs nothing when there is no hold.
const WARM_UP_LIMIT: Duration = Duration::from_secs(30);

/// Run the shim at `shim` once, asking it to do nothing.
///
/// macOS holds the first execution of a bundle it has not seen before for a few
/// seconds (five, measured, whether the bundle is started from a shell or from
/// LaunchServices) and remembers having assessed it after that. Left for the
/// first launch, that hold reads as the app not starting, and is long enough for
/// a launch that watches for the app to conclude it failed. Paying it here, where
/// the user is already waiting on a build, avoids both.
///
/// Best effort: a warm-up that cannot run or does not finish costs the first
/// launch its head start and nothing else.
fn warm_up(shim: &Path) {
    warm_up_within(shim, WARM_UP_LIMIT);
}

fn warm_up_within(shim: &Path, limit: Duration) {
    let Ok(mut child) = Command::new(shim)
        .env(profile_shim::PROBE_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            Ok(Some(_)) | Err(_) => return,
        }
    }
}

/// The `CFBundleVersion` of the app at `bundle`, if it can be read.
pub fn bundle_version(bundle: &Path) -> Option<String> {
    let info = read_info_plist(bundle).ok()?;
    info.get("CFBundleVersion")
        .and_then(Value::as_string)
        .map(str::to_owned)
}

/// The vendor version the wrapper at `wrapper` recorded when it was built.
pub fn built_from_version(wrapper: &Path) -> Option<String> {
    let info = read_info_plist(wrapper).ok()?;
    info.get(info_plist::VENDOR_VERSION_KEY)
        .and_then(Value::as_string)
        .map(str::to_owned)
}

/// Whether a wrapper built from `built_from` is out of step with a vendor now
/// at `current`.
///
/// Defers to the shim's own comparison, because both sides decide this and they
/// must not disagree: the shim rebuilds a wrapper the app would call current, or
/// the other way round, and a launch bounces between them.
pub fn version_drifted(current: &str, built_from: Option<&str>) -> bool {
    profile_shim::should_hand_off(built_from, Some(current))
}

/// Where a wrapper stands against the vendor app it was cloned from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapperState {
    /// There is nothing at the wrapper's path.
    Missing,
    /// It was cloned from another version of the vendor app than is installed
    /// now.
    Stale,
    /// It is there and matches the installed vendor.
    Current,
}

/// Pure: whether `info` records everything the shim needs to hand a launch back
/// to ai-profiles, which is how a wrapper gets itself rebuilt when it is started
/// from the Dock rather than from the app.
fn handoff_recorded(info: &Dictionary) -> bool {
    [
        profile_shim::PROFILE_ID_KEY,
        profile_shim::VENDOR_BUNDLE_KEY,
        profile_shim::HOST_BINARY_KEY,
    ]
    .iter()
    .all(|key| {
        info.get(key)
            .and_then(Value::as_string)
            .is_some_and(|value| !value.is_empty())
    })
}

/// Whether the wrapper at `wrapper` can hand a launch back: it records all
/// three parameters and the binary they name is still there.
///
/// False for every wrapper built before the handoff existed, and for one whose
/// ai-profiles has since moved or been removed. Both need building again, which
/// is why this is part of being stale.
fn handoff_ready(wrapper: &Path) -> bool {
    let Ok(info) = read_info_plist(wrapper) else {
        return false;
    };
    handoff_recorded(&info)
        && info
            .get(profile_shim::HOST_BINARY_KEY)
            .and_then(Value::as_string)
            .is_some_and(|path| Path::new(path).exists())
}

/// The ai-profiles version that built the wrapper at `wrapper`.
fn built_by_version(wrapper: &Path) -> Option<String> {
    let info = read_info_plist(wrapper).ok()?;
    info.get(info_plist::BUILT_BY_KEY)
        .and_then(Value::as_string)
        .map(str::to_owned)
}

/// The state of the wrapper at `wrapper`, given the vendor app it is a clone of
/// (`None` if that is not installed) and the ai-profiles version asking,
/// `built_by`.
///
/// A vendor whose version cannot be read leaves an existing wrapper `Current`,
/// because there is nothing to compare it with and a rebuild would fail anyway.
/// The other two checks have no such excuse: a wrapper that cannot ask for a
/// rebuild, or that an older ai-profiles built, is stale whatever the vendor
/// says — the contents are this app's to keep up to date, and no later launch
/// would put either right on its own.
pub fn state(vendor_bundle: Option<&Path>, wrapper: &Path, built_by: &str) -> WrapperState {
    if !wrapper.exists() {
        return WrapperState::Missing;
    }
    let drifted = vendor_bundle
        .and_then(bundle_version)
        .is_some_and(|current| version_drifted(&current, built_from_version(wrapper).as_deref()));
    let ours = built_by_version(wrapper).as_deref() == Some(built_by);
    if drifted || !ours || !handoff_ready(wrapper) {
        WrapperState::Stale
    } else {
        WrapperState::Current
    }
}

/// Fail unless the signed wrapper verifies and neither of its binaries still
/// claims anything of the vendor's team. The shim is what LaunchServices
/// starts, and the vendor binary is what it becomes, so both count.
fn validate(bundle: &Path, vendor_binary: &Path, team_id: Option<&str>) -> AppResult<()> {
    sign::verify(bundle)?;
    sign::verify(vendor_binary)?;
    for binary in [bundle, vendor_binary] {
        let signature = sign::inspect(binary)?;
        check_entitlements(&signature.entitlements, team_id)
            .map_err(|problem| AppError::Validation(format!("{}: {problem}", binary.display())))?;
    }
    Ok(())
}

/// `Err` describing what is wrong if `entitlements` are not ones a wrapper can
/// launch with.
fn check_entitlements(entitlements: &Dictionary, team_id: Option<&str>) -> Result<(), String> {
    let team_scoped = entitlements::team_scoped_entitlements(entitlements, team_id);
    if !team_scoped.is_empty() {
        return Err(format!(
            "team-scoped entitlements survived signing: {}",
            team_scoped.join(", ")
        ));
    }
    if entitlements.get(entitlements::DISABLE_LIBRARY_VALIDATION) != Some(&Value::Boolean(true)) {
        return Err("library validation is still enabled".to_owned());
    }
    Ok(())
}

/// Clone `vendor` to `staged` with copy-on-write, so a wrapper costs almost no
/// disk until it diverges. A plain copy would silently cost the whole app (over
/// a gigabyte for ChatGPT) per profile, so that is refused rather than allowed.
fn clone_bundle(vendor: &Path, staged: &Path, executable: &str) -> AppResult<()> {
    let directory = staged.parent().unwrap_or(Path::new("/"));
    if fs::metadata(vendor)?.dev() != fs::metadata(directory)?.dev() {
        return Err(AppError::Validation(format!(
            "{} is on a different volume than {}, so cloning it would copy the whole app",
            vendor.display(),
            directory.display()
        )));
    }

    let output = Command::new("/bin/cp")
        .arg("-Rc")
        .arg(vendor)
        .arg(staged)
        .output()?;
    if !output.status.success() {
        return Err(AppError::Io(std::io::Error::other(format!(
            "cloning {} failed ({}): {}",
            vendor.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))));
    }

    let executable = format!("Contents/MacOS/{executable}");
    for relative in ["Contents/Info.plist", executable.as_str()] {
        if fs::read(vendor.join(relative))? != fs::read(staged.join(relative))? {
            return Err(AppError::Validation(format!(
                "the clone's {relative} differs from the original"
            )));
        }
    }
    Ok(())
}

fn read_info_plist(bundle: &Path) -> AppResult<Dictionary> {
    let path = bundle.join("Contents/Info.plist");
    Value::from_file(&path)
        .map_err(|err| AppError::Validation(format!("cannot read {}: {err}", path.display())))?
        .into_dictionary()
        .ok_or_else(|| AppError::Validation(format!("{} is not a dictionary", path.display())))
}

fn write_plist(path: &Path, dictionary: Dictionary) -> AppResult<()> {
    Value::Dictionary(dictionary)
        .to_file_xml(path)
        .map_err(|err| AppError::Validation(format!("cannot write {}: {err}", path.display())))
}

fn utf8(path: &Path) -> AppResult<&str> {
    path.to_str()
        .ok_or_else(|| AppError::Validation(format!("{} is not valid UTF-8", path.display())))
}

/// Name of the hidden directory a wrapper called `stem` is assembled in: dotted
/// so Finder hides it, and without `.app` so LaunchServices does not take it for
/// an app while it still carries the vendor's identifier.
fn staging_name(stem: &str, token: &str) -> String {
    format!(".{stem}.building-{token}")
}

/// A wrapper under construction beside its destination. Whatever it owns is
/// removed on drop unless [`Staging::commit`] moved the bundle into place.
struct Staging {
    /// The bundle being built, a sibling of the destination named by
    /// [`staging_name`].
    bundle: PathBuf,
    /// The entitlements the binaries are signed with, kept outside the bundle
    /// so they are not sealed into it.
    entitlements: PathBuf,
    /// Whether the bundle has been moved to its destination.
    committed: bool,
}

impl Staging {
    fn new(destination: &Path) -> AppResult<Staging> {
        let parent = destination.parent().ok_or_else(|| {
            AppError::Validation(format!("{} has no parent directory", destination.display()))
        })?;
        fs::create_dir_all(parent)?;

        let name = destination
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let token = format!(
            "{}-{}",
            std::process::id(),
            NEXT_STAGING.fetch_add(1, Ordering::Relaxed)
        );
        let bundle = parent.join(staging_name(&name, &token));
        let entitlements =
            std::env::temp_dir().join(format!("ai-profiles-entitlements-{token}.plist"));
        // `cp` would copy *into* a leftover directory instead of creating it.
        if bundle.exists() {
            fs::remove_dir_all(&bundle)?;
        }
        Ok(Staging {
            bundle,
            entitlements,
            committed: false,
        })
    }

    /// Move the finished bundle to `destination`.
    fn commit(mut self, destination: &Path) -> AppResult<()> {
        fs::rename(&self.bundle, destination)?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_dir_all(&self.bundle);
        }
        let _ = fs::remove_file(&self.entitlements);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::Cursor;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::OnceLock;

    use super::*;

    const CLAUDE_ENTITLEMENTS: &str = include_str!("fixtures/claude-entitlements.plist");
    const CLAUDE_TEAM: &str = "Q6L2SF6YDW";
    const CHATGPT_ENTITLEMENTS: &str = include_str!("fixtures/chatgpt-entitlements.plist");
    const CHATGPT_TEAM: &str = "2DC432GLL2";

    /// The team the fake vendor's entitlements claim.
    const FAKE_TEAM: &str = "TESTTEAM01";

    /// A Mach-O that prints each argument it is given, then the value of
    /// `AI_PROFILES_TEST_HOME`: a stand-in for a vendor's executable whose
    /// launch can be observed. Compiled once per test run.
    fn argument_printer() -> &'static [u8] {
        static BINARY: OnceLock<Vec<u8>> = OnceLock::new();
        BINARY.get_or_init(|| {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("printer.rs");
            fs::write(
                &source,
                r#"fn main() {
                    for argument in std::env::args().skip(1) {
                        println!("arg={argument}");
                    }
                    let home = std::env::var("AI_PROFILES_TEST_HOME");
                    println!("env={}", home.as_deref().unwrap_or("unset"));
                }"#,
            )
            .unwrap();
            let binary = dir.path().join("printer");
            let compiler = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
            let status = Command::new(compiler)
                .args(["--edition", "2021", "-O", "-o"])
                .arg(&binary)
                .arg(&source)
                .status()
                .unwrap();
            assert!(status.success(), "could not compile the test executable");
            fs::read(&binary).unwrap()
        })
    }

    fn strings(values: &[&str]) -> Value {
        Value::Array(
            values
                .iter()
                .map(|value| Value::String((*value).to_owned()))
                .collect(),
        )
    }

    /// The entitlements the fake vendor is signed with: team-scoped ones the
    /// wrapper has to lose, alongside one it has to keep.
    fn fake_vendor_entitlements() -> Dictionary {
        let mut entitlements = Dictionary::new();
        entitlements.insert(
            "com.apple.application-identifier".into(),
            Value::String(format!("{FAKE_TEAM}.com.example.fake")),
        );
        entitlements.insert(
            "com.apple.developer.team-identifier".into(),
            Value::String(FAKE_TEAM.into()),
        );
        entitlements.insert(
            "keychain-access-groups".into(),
            strings(&[&format!("{FAKE_TEAM}.com.example.keys")]),
        );
        // Not in the fixed list of team-scoped keys: only its value gives it
        // away. (Avoid non-Apple keys here: AMFI kills an ad-hoc binary that
        // carries one, so the wrapper would not launch.)
        entitlements.insert(
            "com.apple.developer.ubiquity-container-identifiers".into(),
            strings(&[&format!("{FAKE_TEAM}.shared")]),
        );
        entitlements.insert(
            "com.apple.security.cs.allow-jit".into(),
            Value::Boolean(true),
        );
        entitlements
    }

    fn write_dictionary(path: &Path, dictionary: Dictionary) {
        Value::Dictionary(dictionary).to_file_xml(path).unwrap();
    }

    /// `<root>/Fake.app`, signed ad hoc with hardened runtime and entitlements
    /// naming a team, the way a vendor's app is signed apart from the
    /// certificate. Returns the bundle and the entitlements file it was signed
    /// with.
    fn fake_vendor(root: &Path) -> (PathBuf, PathBuf) {
        let bundle = root.join("Fake.app");
        let macos = bundle.join("Contents/MacOS");
        let resources = bundle.join("Contents/Resources");
        fs::create_dir_all(&macos).unwrap();
        fs::create_dir_all(&resources).unwrap();
        fs::write(macos.join("Fake"), argument_printer()).unwrap();
        fs::set_permissions(macos.join("Fake"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(resources.join("electron.icns"), b"vendor icon").unwrap();

        let mut info = Dictionary::new();
        for (key, value) in [
            ("CFBundleName", "Fake"),
            ("CFBundleDisplayName", "Fake"),
            ("CFBundleIdentifier", "com.example.fake"),
            ("CFBundleExecutable", "Fake"),
            ("CFBundlePackageType", "APPL"),
            ("CFBundleIconFile", "electron.icns"),
            ("CFBundleIconName", "Fake"),
            ("CFBundleVersion", "42.1"),
            ("SUPublicEDKey", "sparkle-key"),
        ] {
            info.insert(key.into(), Value::String(value.into()));
        }
        info.insert(
            "CFBundleURLTypes".into(),
            Value::Array(vec![Value::Dictionary(Dictionary::new())]),
        );
        write_dictionary(&bundle.join("Contents/Info.plist"), info);

        let entitlements = root.join("vendor-entitlements.plist");
        write_dictionary(&entitlements, fake_vendor_entitlements());
        let status = Command::new("/usr/bin/codesign")
            .args([
                "--force",
                "--sign",
                "-",
                "--options",
                "runtime",
                "--entitlements",
            ])
            .arg(&entitlements)
            .arg(&bundle)
            .status()
            .unwrap();
        assert!(status.success(), "could not sign the fake vendor");
        (bundle, entitlements)
    }

    fn request<'a>(
        vendor: &'a Path,
        destination: &'a Path,
        home: Option<&'a Path>,
    ) -> WrapperRequest<'a> {
        WrapperRequest {
            vendor_bundle: vendor,
            destination,
            identifier: "app.ai-profiles.fake.profile.1",
            display_name: "Fake (Work)",
            icon: b"badged icon",
            user_data_dir: Path::new("/data/gui data"),
            profile_id: "1",
            host_binary: Path::new("/Applications/ai-profiles.app/Contents/MacOS/ai-profiles"),
            built_by: BUILT_BY,
            config_env: home.map(|home| ("AI_PROFILES_TEST_HOME", home)),
        }
    }

    /// Every file under `dir`, by relative path, with its bytes.
    fn snapshot(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(root, &path, files);
                } else {
                    files.insert(
                        path.strip_prefix(root).unwrap().to_owned(),
                        fs::read(&path).unwrap(),
                    );
                }
            }
        }
        let mut files = BTreeMap::new();
        walk(dir, dir, &mut files);
        files
    }

    fn parse(xml: &str) -> Dictionary {
        Value::from_reader(Cursor::new(xml.as_bytes()))
            .unwrap()
            .into_dictionary()
            .unwrap()
    }

    #[test]
    fn build_makes_a_signed_wrapper_that_launches_the_vendor_binary() {
        let dir = tempfile::tempdir().unwrap();
        let (vendor, _) = fake_vendor(dir.path());
        let destination = dir.path().join("Fake (Work).app");
        let home = Path::new("/data/cli-config");

        build(&request(&vendor, &destination, Some(home))).unwrap();

        let macos = destination.join("Contents/MacOS");
        assert!(macos.join("Fake").is_file(), "shim under the original name");
        assert!(macos.join("Fake.bin").is_file(), "vendor binary set aside");
        assert_eq!(
            fs::read(destination.join("Contents/Resources/AppIcon.icns")).unwrap(),
            b"badged icon"
        );

        let info = read_info_plist(&destination).unwrap();
        let text = |key: &str| info.get(key).and_then(Value::as_string).map(str::to_owned);
        assert_eq!(
            text("CFBundleIdentifier").as_deref(),
            Some("app.ai-profiles.fake.profile.1")
        );
        assert_eq!(text("CFBundleDisplayName").as_deref(), Some("Fake (Work)"));
        assert_eq!(text("CFBundleName").as_deref(), Some("Fake"));
        assert_eq!(text("CFBundleIconFile").as_deref(), Some("AppIcon"));
        assert_eq!(
            text(profile_shim::USER_DATA_DIR_KEY).as_deref(),
            Some("/data/gui data")
        );
        assert_eq!(
            text(info_plist::VENDOR_VERSION_KEY).as_deref(),
            Some("42.1")
        );
        assert!(!info.contains_key("CFBundleIconName"));
        assert!(!info.contains_key("CFBundleURLTypes"));
        assert_eq!(
            info.get("SUEnableAutomaticChecks"),
            Some(&Value::Boolean(false))
        );

        sign::verify(&destination).unwrap();

        // What a Dock click does: launch with no arguments of its own (a
        // couple are added here to see them pass through in order).
        let output = Command::new(macos.join("Fake"))
            .args(["--flag", "two words"])
            .env_remove("AI_PROFILES_TEST_HOME")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "wrapper did not launch: {} {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "arg=--user-data-dir=/data/gui data\n\
             arg=--flag\n\
             arg=two words\n\
             env=/data/cli-config\n"
        );
    }

    #[test]
    fn build_signs_both_binaries_without_the_vendors_team_scoped_entitlements() {
        let dir = tempfile::tempdir().unwrap();
        let (vendor, _) = fake_vendor(dir.path());
        let destination = dir.path().join("Fake (Work).app");

        build(&request(&vendor, &destination, None)).unwrap();

        // Sanity: the fake vendor really does start out with a team.
        let original = sign::inspect(&vendor).unwrap();
        assert!(
            !entitlements::team_scoped_entitlements(&original.entitlements, Some(FAKE_TEAM))
                .is_empty()
        );

        for binary in [
            destination.clone(),
            destination.join("Contents/MacOS/Fake.bin"),
        ] {
            let signed = sign::inspect(&binary).unwrap().entitlements;
            let name = binary.display();
            assert_eq!(
                entitlements::team_scoped_entitlements(&signed, Some(FAKE_TEAM)),
                Vec::<String>::new(),
                "{name}"
            );
            assert_eq!(
                signed.get(entitlements::DISABLE_LIBRARY_VALIDATION),
                Some(&Value::Boolean(true)),
                "{name}"
            );
            assert_eq!(
                signed.get("com.apple.security.cs.allow-jit"),
                Some(&Value::Boolean(true)),
                "{name}: unrelated entitlements are kept"
            );
            assert!(
                !signed.contains_key("com.apple.developer.ubiquity-container-identifiers"),
                "{name}: a key that only names the team in its value goes too"
            );
        }
    }

    #[test]
    fn build_never_writes_to_the_vendor_bundle() {
        let dir = tempfile::tempdir().unwrap();
        let (vendor, _) = fake_vendor(dir.path());
        let before = snapshot(&vendor);
        let destination = dir.path().join("Fake (Work).app");

        build(&request(&vendor, &destination, None)).unwrap();

        assert_eq!(snapshot(&vendor), before);
        sign::verify(&vendor).unwrap();
    }

    #[test]
    fn build_refuses_a_destination_that_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let (vendor, _) = fake_vendor(dir.path());
        let destination = dir.path().join("Fake (Work).app");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("keep"), b"mine").unwrap();

        assert!(build(&request(&vendor, &destination, None)).is_err());

        assert_eq!(fs::read(destination.join("keep")).unwrap(), b"mine");
    }

    #[test]
    fn a_failed_build_leaves_nothing_behind() {
        let dir = tempfile::tempdir().unwrap();
        let (vendor, _) = fake_vendor(dir.path());
        let destination = dir.path().join("Fake (Work).app");
        // `cp` cannot read this, so the clone fails after it has started.
        let unreadable = vendor.join("Contents/Resources/electron.icns");
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&unreadable).is_ok() {
            // Running as root: permissions do not stop it, so nothing fails.
            eprintln!("skipping; the file is readable despite mode 000");
            return;
        }

        let result = build(&request(&vendor, &destination, None));

        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(result.is_err());
        assert!(!destination.exists());
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains("building"))
            .collect();
        assert_eq!(leftovers, Vec::<String>::new());
    }

    #[test]
    fn staging_removes_what_it_owns_unless_it_was_committed() {
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("Fake (Work).app");

        let abandoned = Staging::new(&destination).unwrap();
        fs::create_dir(&abandoned.bundle).unwrap();
        fs::write(&abandoned.entitlements, b"x").unwrap();
        let (bundle, entitlements) = (abandoned.bundle.clone(), abandoned.entitlements.clone());
        drop(abandoned);
        assert!(!bundle.exists() && !entitlements.exists());

        let finished = Staging::new(&destination).unwrap();
        fs::create_dir(&finished.bundle).unwrap();
        fs::write(&finished.entitlements, b"x").unwrap();
        let entitlements = finished.entitlements.clone();
        finished.commit(&destination).unwrap();
        assert!(destination.is_dir(), "committed bundle is in place");
        assert!(!entitlements.exists());
    }

    #[test]
    fn staging_is_hidden_and_not_an_app_bundle() {
        let name = staging_name("Claude (Work)", "42-0");

        assert!(name.starts_with('.'), "{name}");
        assert!(!name.ends_with(".app"), "{name}");
    }

    #[test]
    fn validation_rejects_a_wrapper_that_kept_a_team_scoped_entitlement() {
        let dir = tempfile::tempdir().unwrap();
        let (vendor, unstripped) = fake_vendor(dir.path());
        let destination = dir.path().join("Fake (Work).app");
        build(&request(&vendor, &destination, None)).unwrap();
        let vendor_binary = destination.join("Contents/MacOS/Fake.bin");
        validate(&destination, &vendor_binary, Some(FAKE_TEAM)).unwrap();

        // Re-sign everything with the vendor's entitlements: the signature is
        // intact, only the entitlements are wrong.
        sign::sign(&vendor_binary, &unstripped).unwrap();
        sign::sign(&destination, &unstripped).unwrap();

        let error = validate(&destination, &vendor_binary, Some(FAKE_TEAM)).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("team-scoped entitlements survived"),
            "{error}"
        );
        assert!(
            error.to_string().contains("keychain-access-groups"),
            "{error}"
        );
    }

    #[test]
    fn entitlements_check_rejects_the_real_vendors_and_accepts_them_stripped() {
        for (xml, team) in [
            (CLAUDE_ENTITLEMENTS, CLAUDE_TEAM),
            (CHATGPT_ENTITLEMENTS, CHATGPT_TEAM),
        ] {
            let vendor = parse(xml);
            assert!(check_entitlements(&vendor, Some(team)).is_err());

            let stripped = entitlements::strip_team_entitlements(&vendor, Some(team));
            assert_eq!(check_entitlements(&stripped, Some(team)), Ok(()));
        }
    }

    #[test]
    fn entitlements_check_requires_library_validation_to_be_off() {
        let mut stripped =
            entitlements::strip_team_entitlements(&parse(CLAUDE_ENTITLEMENTS), Some(CLAUDE_TEAM));
        stripped.remove(entitlements::DISABLE_LIBRARY_VALIDATION);

        let problem = check_entitlements(&stripped, Some(CLAUDE_TEAM)).unwrap_err();

        assert!(problem.contains("library validation"), "{problem}");
    }

    /// The ai-profiles version the tests build wrappers with.
    const BUILT_BY: &str = "1.3.0";

    /// `<dir>/<name>` as a bundle whose `Info.plist` holds `entries`.
    fn bundle_with_info(dir: &Path, name: &str, entries: &[(&str, &str)]) -> PathBuf {
        let bundle = dir.join(name);
        fs::create_dir_all(bundle.join("Contents")).unwrap();
        let mut info = Dictionary::new();
        for (key, value) in entries {
            info.insert((*key).to_owned(), Value::String((*value).to_owned()));
        }
        write_dictionary(&bundle.join("Contents/Info.plist"), info);
        bundle
    }

    #[test]
    fn a_wrapper_has_drifted_unless_built_from_the_installed_vendor_version() {
        assert!(!version_drifted("2.2553.1", Some("2.2553.1")), "equal");
        assert!(
            version_drifted("2.2554.0", Some("2.2553.1")),
            "vendor newer"
        );
        assert!(
            version_drifted("2.2552.0", Some("2.2553.1")),
            "vendor older"
        );
        assert!(version_drifted("2.2553.1", None), "no recorded version");
    }

    /// A shell script at `path` that runs `body`.
    fn write_script(path: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn warming_up_runs_the_shim_asking_it_to_do_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let record = dir.path().join("record");
        let shim = dir.path().join("shim");
        write_script(
            &shim,
            &format!(
                "printf '%s' \"${{{}-unset}}\" > '{}'\n",
                profile_shim::PROBE_ENV,
                record.display()
            ),
        );

        // Running a file just written can meet "text file busy" while another
        // test thread is forking; the warm-up ignores that, so try again.
        for _ in 0..40 {
            warm_up_within(&shim, Duration::from_secs(10));
            if record.exists() {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }

        assert_eq!(fs::read_to_string(&record).unwrap(), "1");
    }

    #[test]
    fn a_warm_up_that_does_not_finish_is_given_up_on_and_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let shim = dir.path().join("shim");
        write_script(&shim, "sleep 30\n");

        let started = Instant::now();
        warm_up_within(&shim, Duration::from_millis(300));

        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn a_warm_up_of_something_that_cannot_run_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        warm_up_within(&dir.path().join("no-such-shim"), Duration::from_secs(1));
    }

    /// The handoff keys a wrapper needs to count as current, pointing at a host
    /// binary that exists: `dir` must be the temp dir `host` was written into.
    fn handoff_entries(host: &str) -> Vec<(&str, &str)> {
        vec![
            (profile_shim::PROFILE_ID_KEY, "profile-1"),
            (profile_shim::VENDOR_BUNDLE_KEY, "/Applications/Vendor.app"),
            (profile_shim::HOST_BINARY_KEY, host),
            (info_plist::BUILT_BY_KEY, BUILT_BY),
        ]
    }

    /// An ai-profiles binary for a wrapper to point at. Only its existence
    /// matters here.
    fn write_host_binary(dir: &Path) -> String {
        let host = dir.join("ai-profiles");
        fs::write(&host, b"host").unwrap();
        host.display().to_string()
    }

    /// `<dir>/<name>` as a wrapper of `version` that can ask for a rebuild.
    fn wrapper_bundle(dir: &Path, name: &str, version: &str, host: &str) -> PathBuf {
        let mut entries = vec![(info_plist::VENDOR_VERSION_KEY, version)];
        entries.extend(handoff_entries(host));
        bundle_with_info(dir, name, &entries)
    }

    #[test]
    fn state_compares_the_installed_vendor_with_what_the_wrapper_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let host = write_host_binary(dir.path());
        let vendor = bundle_with_info(dir.path(), "Vendor.app", &[("CFBundleVersion", "2.0")]);
        let state_of = |wrapper: &Path| state(Some(&vendor), wrapper, BUILT_BY);

        let current = wrapper_bundle(dir.path(), "Current.app", "2.0", &host);
        assert_eq!(state_of(&current), WrapperState::Current);

        let older = wrapper_bundle(dir.path(), "Older.app", "1.9", &host);
        assert_eq!(state_of(&older), WrapperState::Stale);

        let unrecorded = bundle_with_info(dir.path(), "Unrecorded.app", &handoff_entries(&host));
        assert_eq!(state_of(&unrecorded), WrapperState::Stale);

        assert_eq!(
            state_of(&dir.path().join("Missing.app")),
            WrapperState::Missing
        );
    }

    #[test]
    fn state_leaves_an_existing_wrapper_alone_when_the_vendor_cannot_be_read() {
        let dir = tempfile::tempdir().unwrap();
        let host = write_host_binary(dir.path());
        let wrapper = wrapper_bundle(dir.path(), "Wrapper.app", "2.0", &host);

        let not_installed = None;
        let unreadable = Some(dir.path().join("NoVendor.app"));
        assert_eq!(
            state(not_installed, &wrapper, BUILT_BY),
            WrapperState::Current
        );
        assert_eq!(
            state(unreadable.as_deref(), &wrapper, BUILT_BY),
            WrapperState::Current
        );
        // Nothing to compare with does not make up for nothing being there.
        assert_eq!(
            state(not_installed, &dir.path().join("Missing.app"), BUILT_BY),
            WrapperState::Missing
        );
    }

    #[test]
    fn a_wrapper_that_cannot_ask_for_a_rebuild_is_stale_whatever_the_vendor_says() {
        let dir = tempfile::tempdir().unwrap();
        let host = write_host_binary(dir.path());
        let vendor = bundle_with_info(dir.path(), "Vendor.app", &[("CFBundleVersion", "2.0")]);

        // Built before the handoff existed: the version matches, but there is
        // no way for it to notice the next time it does not.
        let before = bundle_with_info(
            dir.path(),
            "Before.app",
            &[(info_plist::VENDOR_VERSION_KEY, "2.0")],
        );
        assert_eq!(state(Some(&vendor), &before, BUILT_BY), WrapperState::Stale);
        assert_eq!(state(None, &before, BUILT_BY), WrapperState::Stale);

        // One key short is no better than none.
        for dropped in [
            profile_shim::PROFILE_ID_KEY,
            profile_shim::VENDOR_BUNDLE_KEY,
            profile_shim::HOST_BINARY_KEY,
        ] {
            let mut entries = vec![(info_plist::VENDOR_VERSION_KEY, "2.0")];
            entries.extend(
                handoff_entries(&host)
                    .into_iter()
                    .filter(|(key, _)| *key != dropped),
            );
            let partial = bundle_with_info(dir.path(), &format!("No{dropped}.app"), &entries);
            assert_eq!(
                state(Some(&vendor), &partial, BUILT_BY),
                WrapperState::Stale,
                "{dropped}"
            );
        }

        // The host binary it names has moved or been removed.
        let gone = wrapper_bundle(
            dir.path(),
            "Gone.app",
            "2.0",
            &dir.path().join("not-there").display().to_string(),
        );
        assert_eq!(state(Some(&vendor), &gone, BUILT_BY), WrapperState::Stale);
    }

    #[test]
    fn a_wrapper_an_older_ai_profiles_built_is_stale_even_in_step_with_the_vendor() {
        let dir = tempfile::tempdir().unwrap();
        let host = write_host_binary(dir.path());
        let vendor = bundle_with_info(dir.path(), "Vendor.app", &[("CFBundleVersion", "2.0")]);
        let wrapper = wrapper_bundle(dir.path(), "Wrapper.app", "2.0", &host);

        // A wrapper carries a copy of the shim, so it is only as new as the
        // ai-profiles that wrote it.
        assert_eq!(
            state(Some(&vendor), &wrapper, BUILT_BY),
            WrapperState::Current
        );
        assert_eq!(state(Some(&vendor), &wrapper, "1.4.0"), WrapperState::Stale);
        assert_eq!(state(None, &wrapper, "1.4.0"), WrapperState::Stale);
    }

    /// Opt-in: builds wrappers from whichever vendor apps are installed, into a
    /// temp dir (never `/Applications`), and checks what has to hold for them
    /// to launch. Gated behind AI_PROFILES_E2E=1 because it needs those apps
    /// and signs a gigabyte or so of them.
    #[test]
    fn builds_wrappers_from_the_installed_vendor_apps() {
        if std::env::var("AI_PROFILES_E2E").is_err() {
            eprintln!("skipping; set AI_PROFILES_E2E=1 to run");
            return;
        }
        for spec in [&crate::app_kind::CLAUDE, &crate::app_kind::CODEX] {
            let Some(app) = crate::paths::resolve_gui_app(spec) else {
                eprintln!("{} not installed; skipping", spec.display_name);
                continue;
            };
            let dir = tempfile::tempdir().unwrap();
            let destination = dir.path().join(format!("{} (E2E).app", spec.display_name));
            let icon = crate::launchers::icons::render_icns("#7C3AED", &app.bundle_path).unwrap();
            let vendor_before = fs::read(app.bundle_path.join("Contents/Info.plist")).unwrap();
            let data = dir.path().join("gui-data");
            let home = dir.path().join("cli-config");

            build(&WrapperRequest {
                vendor_bundle: &app.bundle_path,
                destination: &destination,
                identifier: "app.ai-profiles.e2e.profile.1",
                display_name: "E2E Profile",
                icon: &icon,
                user_data_dir: &data,
                profile_id: "e2e-profile-1",
                host_binary: &std::env::current_exe().unwrap(),
                built_by: BUILT_BY,
                config_env: spec
                    .gui_exports_config_env
                    .then_some((spec.cli_config_env, home.as_path())),
            })
            .unwrap_or_else(|err| panic!("{}: {err}", spec.display_name));

            let macos = destination.join("Contents/MacOS");
            let vendor_binary = macos.join(format!("{}.bin", app.macos_exec));
            assert!(vendor_binary.is_file(), "{}", spec.display_name);
            assert_ne!(
                fs::read(macos.join(app.macos_exec)).unwrap(),
                fs::read(&vendor_binary).unwrap(),
                "{}: the executable should be the shim",
                spec.display_name
            );

            // The seal covers all the nested vendor code too.
            let deep = Command::new("/usr/bin/codesign")
                .args(["--verify", "--deep", "--strict"])
                .arg(&destination)
                .output()
                .unwrap();
            assert!(
                deep.status.success(),
                "{}: {}",
                spec.display_name,
                String::from_utf8_lossy(&deep.stderr)
            );

            // And the vendor's own app is exactly as it was.
            assert_eq!(
                fs::read(app.bundle_path.join("Contents/Info.plist")).unwrap(),
                vendor_before
            );
            sign::verify(&app.bundle_path).unwrap();
        }
    }
}

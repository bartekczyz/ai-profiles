//! Starting a desktop app, or a profile of it, exactly once.
//!
//! Claude does NOT enforce one instance per data directory: `open -n` always
//! spawns a brand-new process, and two instances happily share the same
//! `--user-data-dir`. To give each entry (the stock default and every managed
//! profile) "exactly one window, all coexisting" behaviour, we track the
//! running instances ourselves: before launching we scan for a main process
//! already bound to the target data dir, and focus it instead of spawning a
//! duplicate.
//!
//! A profile with a wrapper (see [`crate::launchers::wrapper`]) is opened
//! through it, and never becomes unlaunchable because of it: whatever is wrong
//! with the wrapper, the profile is started the stock way instead and the caller
//! is told why.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use profile_shim::VENDOR_BINARY_SUFFIX;

use crate::app_kind::{AppKind, AppSpec};
use crate::error::{AppError, AppResult};
use crate::launchers::gui;
use crate::launchers::wrapper::{self, WrapperState};
use crate::paths::{
    cli_config_dir, gui_launcher_path, profile_dir, resolve_gui_app, ResolvedGuiApp,
};
use crate::profiles::Profile;

/// Find the PID of the *main* GUI process (exec `<gui_macos_exec>`) bound to
/// `data_dir` in `ps` output, or `None`.
///
/// Every running instance is one `…/Contents/MacOS/<exec>
/// --user-data-dir=<dir>` process, or, for a profile with a wrapper, `…/<exec>.bin`
/// in the same place: the vendor binary the wrapper's shim starts. Helper
/// processes (renderer, GPU, network, …) carry the same flag, but they run a
/// different executable (`…/MacOS/<exec> Helper`) and always have trailing
/// arguments. Requiring the line to end with `…/Contents/MacOS/<exec>
/// --user-data-dir=<dir>` therefore matches only the main process and rejects
/// helpers, the crashpad handler, and instances bound to any other data dir.
pub fn find_running_pid(ps_output: &str, data_dir: &str, gui_macos_exec: &str) -> Option<i32> {
    first_pid_ending_with(
        ps_output,
        &[
            stock_suffix(data_dir, gui_macos_exec),
            wrapper_suffix(data_dir, gui_macos_exec),
        ],
    )
}

/// Every PID [`find_running_pid`] would pick from, in `ps` order: an app
/// doesn't keep to one instance per data dir.
pub fn find_running_pids(ps_output: &str, data_dir: &str, gui_macos_exec: &str) -> Vec<i32> {
    pids_ending_with(
        ps_output,
        &[
            stock_suffix(data_dir, gui_macos_exec),
            wrapper_suffix(data_dir, gui_macos_exec),
        ],
    )
}

/// As [`find_running_pid`], but only for a profile running from its wrapper.
pub fn find_running_wrapper_pid(
    ps_output: &str,
    data_dir: &str,
    gui_macos_exec: &str,
) -> Option<i32> {
    first_pid_ending_with(ps_output, &[wrapper_suffix(data_dir, gui_macos_exec)])
}

/// The end of the command line of the stock app's main process.
fn stock_suffix(data_dir: &str, gui_macos_exec: &str) -> String {
    format!("/Contents/MacOS/{gui_macos_exec} --user-data-dir={data_dir}")
}

/// The end of the command line of a wrapper's main process: the vendor binary
/// the shim started, next to the shim.
fn wrapper_suffix(data_dir: &str, gui_macos_exec: &str) -> String {
    format!("/Contents/MacOS/{gui_macos_exec}{VENDOR_BINARY_SUFFIX} --user-data-dir={data_dir}")
}

/// The PID of the first process in `ps_output` whose command line ends with one
/// of `suffixes`.
pub(crate) fn first_pid_ending_with(ps_output: &str, suffixes: &[String]) -> Option<i32> {
    pids_ending_with(ps_output, suffixes).into_iter().next()
}

/// The PIDs of the processes in `ps_output` whose command line ends with one
/// of `suffixes`, in `ps` order.
pub(crate) fn pids_ending_with(ps_output: &str, suffixes: &[String]) -> Vec<i32> {
    ps_output
        .lines()
        .filter_map(|line| {
            let (pid, command) = line.trim_start().split_once(char::is_whitespace)?;
            let command = command.trim_end();
            suffixes
                .iter()
                .any(|suffix| command.ends_with(suffix.as_str()))
                .then(|| pid.parse::<i32>().ok())
                .flatten()
        })
        .collect()
}

/// Every running process, one `pid command` line each.
pub(crate) fn process_list() -> AppResult<String> {
    let output = Command::new("ps")
        .args(["-ax", "-o", "pid=,command="])
        .output()
        .map_err(AppError::Io)?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Scan running processes for a main GUI process bound to `data_dir`.
fn running_pid(data_dir: &str, gui_macos_exec: &str) -> AppResult<Option<i32>> {
    Ok(find_running_pid(&process_list()?, data_dir, gui_macos_exec))
}

/// The PID of `profile`'s wrapper, if the profile is running from one.
///
/// Every executable name the app has gone by is tried: a wrapper's is the one of
/// the vendor app it was cloned from, and that app may have been renamed since.
pub fn running_wrapper(profile: &Profile) -> AppResult<Option<i32>> {
    if !profile.distinct_dock_icon {
        return Ok(None);
    }
    let spec = profile.app.spec();
    let data_dir = profile_dir(&profile.id)?
        .join("gui-data")
        .display()
        .to_string();
    let processes = process_list()?;
    Ok(spec.gui_bundle_candidates.iter().find_map(|candidate| {
        find_running_wrapper_pid(&processes, &data_dir, candidate.macos_exec)
    }))
}

/// Surface the already-running instance that owns `pid`: ask it to reopen a
/// window, then bring it to the foreground.
///
/// The reopen step matters because closing a Claude window does not quit the
/// app on macOS — the process keeps running with no windows. Plain activation
/// would bring that windowless process forward but show nothing. Sending it
/// the `reopen` Apple event (`aevt`/`rapp`) — exactly what clicking the Dock
/// icon does — makes it recreate its window. Addressing the event to a
/// specific PID keeps it scoped to the right instance even when several Claude
/// processes (other profiles) are running under the shared bundle id.
///
/// Best-effort: a missing process or a refused step is ignored — the caller
/// has already decided not to spawn a duplicate.
#[cfg(target_os = "macos")]
pub fn focus_pid(pid: i32) {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventSendOptions};

    // 'aevt'/'rapp' — the standard reopen-application event. Reopen is exempt
    // from the Automation (TCC) consent prompt, so this is silent.
    const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
    const REOPEN_APPLICATION: u32 = u32::from_be_bytes(*b"rapp");
    const AUTO_GENERATE_RETURN_ID: i16 = -1;
    const ANY_TRANSACTION_ID: i32 = 0;
    const TIMEOUT_SECONDS: f64 = 5.0;

    let target = NSAppleEventDescriptor::descriptorWithProcessIdentifier(pid);
    let reopen = NSAppleEventDescriptor::appleEventWithEventClass_eventID_targetDescriptor_returnID_transactionID(
        CORE_EVENT_CLASS,
        REOPEN_APPLICATION,
        Some(&target),
        AUTO_GENERATE_RETURN_ID,
        ANY_TRANSACTION_ID,
    );
    let _ = reopen
        .sendEventWithOptions_timeout_error(NSAppleEventSendOptions::NoReply, TIMEOUT_SECONDS);

    // Activating another application requires no entitlement and triggers no
    // TCC prompt.
    #[allow(deprecated)]
    if let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
        app.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn focus_pid(_pid: i32) {}

/// Focus the running GUI instance bound to `data_dir`, or run `launch` when
/// none is running. This is the single-instance gate shared by the default
/// entry and the managed profiles.
///
/// `focus` is handed the PID of the running instance, to bring it forward. It is
/// a parameter because AppKit wants that done on the main thread, and a launch
/// that waits for its app to come up cannot run there.
///
/// If `spec`'s GUI bundle can't be resolved on disk, nothing could possibly
/// be running under it, so this skips straight to `launch` — which will
/// itself surface a clear "not installed" error.
pub fn focus_or_launch<F, G>(data_dir: &str, spec: &AppSpec, focus: G, launch: F) -> AppResult<()>
where
    F: FnOnce() -> AppResult<()>,
    G: FnOnce(i32),
{
    if let Some(resolved) = crate::paths::resolve_gui_app(spec) {
        if let Some(pid) = running_pid(data_dir, resolved.macos_exec)? {
            focus(pid);
            return Ok(());
        }
    }
    launch()
}

/// Launch a fresh GUI instance bound to `data_dir` via
/// `open -n -a <bundle path> --args --user-data-dir=<dir>` — the same
/// incantation the per-profile launcher `.app` bundles use, just invoked
/// directly. Used by the default entry, which has no launcher bundle of its
/// own, and as the stock way of starting a profile whose wrapper cannot be used.
///
/// `config_env` is the profile's config home to start the app with
/// ([`AppSpec::cli_config_env`] at its `cli-config` dir): Codex reads its
/// account from it and Claude's Code tab its config and history, so without it
/// a profile would open on the stock ones. `None` for the default entry, which
/// is the stock app on its own home. `open` hands its environment on to the app
/// it starts, so the config homes ai-profiles itself was started with are taken
/// out first (see [`open_command`]).
///
/// Launches by resolved absolute bundle path rather than a registered app
/// name, so it keeps working across a bundle rename (as happened when OpenAI
/// replaced Codex.app with ChatGPT.app) without depending on LaunchServices
/// having a name-to-bundle mapping for it.
pub fn open_new_instance(
    data_dir: &str,
    spec: &AppSpec,
    config_env: Option<(&str, &Path)>,
) -> AppResult<()> {
    let resolved = crate::paths::resolve_gui_app(spec)
        .ok_or_else(|| AppError::Validation(format!("{} isn't installed", spec.display_name)))?;
    let mut command = open_command();
    command
        .arg("-n")
        .arg("-a")
        .arg(&resolved.bundle_path)
        .arg("--args")
        .arg(format!("--user-data-dir={data_dir}"));
    if let Some((name, value)) = config_env {
        command.env(name, value);
    }
    let status = command.status().map_err(AppError::Io)?;
    if !status.success() {
        return Err(AppError::Validation(format!(
            "`open -n -a {} --args --user-data-dir={data_dir}` exited with status {status}",
            resolved.bundle_path.display()
        )));
    }
    Ok(())
}

/// `open`, without every app's config-home variable (`CLAUDE_CONFIG_DIR`,
/// `CODEX_HOME`) in what it passes on. Every app ai-profiles starts is started
/// through this, so no launch can leave them in.
///
/// `open` hands the app it starts ai-profiles' own environment, and ai-profiles
/// may have been started with one of these set (from a shell running under some
/// profile, say). The app would then run on that profile's config: the stock
/// app, which is given none of its own, always, and a launcher that sets none of
/// its own too. A profile's own is set on the command afterwards, and wins.
fn open_command() -> Command {
    let mut command = Command::new("open");
    for kind in [AppKind::Claude, AppKind::Codex] {
        command.env_remove(kind.spec().cli_config_env);
    }
    command
}

/// Open the app at `bundle` the way a click on its Dock tile does: no `-n`, no
/// arguments. That a wrapper starts right this way is the point of it.
fn open_bundle(bundle: &Path) -> AppResult<()> {
    let output = open_command().arg(bundle).output().map_err(AppError::Io)?;
    if !output.status.success() {
        return Err(AppError::Validation(format!(
            "`open {}` exited with status {}: {}",
            bundle.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

/// Which launcher a launch starts out with, decided before anything is touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    /// The profile has no wrapper: open its script launcher, as ever.
    ScriptLauncher,
    /// Open the wrapper as it is.
    Wrapper,
    /// The wrapper is missing, or was cloned from another version of the vendor
    /// app than is installed: rebuild it, then open it. A stale wrapper would
    /// otherwise keep running the old version without a word.
    RebuildWrapper,
}

/// Pure: the launcher to start `distinct_dock_icon`'s profile with, given where
/// its wrapper stands.
pub fn route(distinct_dock_icon: bool, wrapper: WrapperState) -> Route {
    match (distinct_dock_icon, wrapper) {
        (false, _) => Route::ScriptLauncher,
        (true, WrapperState::Current) => Route::Wrapper,
        (true, WrapperState::Missing | WrapperState::Stale) => Route::RebuildWrapper,
    }
}

/// Why a profile that asks for a wrapper was started the stock way instead.
///
/// Never a reason to turn the setting off: a wrapper that failed once may work
/// the next time, and switching it off for the user would be a silent change of
/// configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bypass {
    /// The wrapper had to be rebuilt and that failed.
    RebuildFailed(String),
    /// macOS refused to open the wrapper.
    OpenFailed(String),
    /// macOS opened it, but no process of the profile's ever showed up.
    NeverStarted,
    /// The profile's process showed up and then went away again.
    ExitedEarly,
}

impl fmt::Display for Bypass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Bypass::RebuildFailed(detail) => {
                write!(
                    formatter,
                    "The launcher couldn't be rebuilt: {}",
                    brief(detail)
                )
            }
            Bypass::OpenFailed(detail) => {
                write!(
                    formatter,
                    "macOS couldn't open the launcher: {}",
                    brief(detail)
                )
            }
            Bypass::NeverStarted => write!(formatter, "The launcher didn't start."),
            Bypass::ExitedEarly => write!(formatter, "The launcher quit right after starting."),
        }
    }
}

/// The longest a cause is quoted at in a [`Bypass`] message.
const BRIEF_MAX_CHARS: usize = 140;

/// Pure: the first line of `text`, cut to [`BRIEF_MAX_CHARS`]. Tool errors run
/// to paragraphs, and the message ends up in a one-line notice.
fn brief(text: &str) -> String {
    let line = text.lines().next().unwrap_or_default().trim();
    if line.chars().count() <= BRIEF_MAX_CHARS {
        return line.to_owned();
    }
    let cut: String = line.chars().take(BRIEF_MAX_CHARS).collect();
    format!("{}…", cut.trim_end())
}

/// How far apart the looks at a wrapper that was just opened are.
const LOOK_INTERVAL: Duration = Duration::from_millis(100);

/// How many looks (about five seconds) it may take for anything of a wrapper
/// that `open` said it started to show up at all. LaunchServices had started the
/// shim within a few tens of milliseconds every time this was measured.
const APPEAR_LOOKS: usize = 50;

/// How many looks (about a minute) a wrapper may be seen starting, without its
/// app ever coming up, before it is left to get on with it.
const LAUNCH_LOOKS: usize = 600;

/// How many looks in a row (about a second and a half) the app has to be seen
/// running before it counts as started.
const STEADY_LOOKS: usize = 15;

/// How many looks (about twenty seconds) a launch with no wrapper of its own is
/// given to show up. Long enough for a cold start of an Electron app, short
/// enough that a launch which never happens stops being reported as under way.
const SETTLE_LOOKS: usize = 200;

/// Whether a GUI instance bound to `data_dir` is running — or cannot be told
/// apart from one, because the process list would not read. As in [`sighting`],
/// no telling counts as running: waiting forever would be worse.
fn seems_up(data_dir: &str, gui_macos_exec: &str) -> bool {
    !matches!(running_pid(data_dir, gui_macos_exec), Ok(None))
}

/// Keep looking, `pause` apart, until `up` says the app is there or `looks`
/// looks have gone by.
///
/// Unlike [`watch_start`] this has no verdict to give: `open` has already
/// accepted the launch, and an app that is slow to appear is no reason to start
/// a second one. All it decides is how long the caller goes on treating the
/// launch as under way — which is what keeps "Opening" on the button until
/// there is something to open.
fn wait_until_up(pause: Duration, looks: usize, mut up: impl FnMut() -> bool) {
    for _ in 0..looks {
        thread::sleep(pause);
        if up() {
            return;
        }
    }
}

/// Wait for a launch of `spec`'s stock app on `data_dir` to show up.
///
/// The counterpart of [`open_new_instance`], which returns as soon as `open`
/// has handed the request to LaunchServices — several seconds before any window
/// exists. Best effort: nothing here fails, and the app is on its way either
/// way.
pub fn wait_for_new_instance(data_dir: &str, spec: &AppSpec) {
    let Some(resolved) = resolve_gui_app(spec) else {
        return;
    };
    wait_until_up(LOOK_INTERVAL, SETTLE_LOOKS, || {
        seems_up(data_dir, resolved.macos_exec)
    });
}

/// What one look at the running processes turned up for a wrapper that was just
/// opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sighting {
    /// The app itself is running: the vendor binary, on the profile's data dir.
    main: bool,
    /// Something is running from inside the wrapper: the shim, before it has
    /// started the vendor binary, or the app and its helpers.
    any: bool,
}

/// Decides, look by look, whether a wrapper that was just opened started.
///
/// `open` says it succeeded as soon as LaunchServices has started the shim,
/// whether or not the app ever comes up: with the vendor binary missing, or an
/// `Info.plist` that is broken, the shim is gone within 100 ms and `open` still
/// exits 0. So the only way to tell is to look. But looking for the app alone is
/// not enough. The first time macOS is asked to run a bundle it has not seen it
/// holds it for some seconds, with the shim already running and the app not there
/// yet. That is a start in progress, and answering it with the stock app would
/// leave two instances on one data dir once the wrapper came up. What counts
/// against a wrapper is therefore the shim going away without the app having come
/// up, or the app going away shortly after it had.
#[derive(Debug, Default)]
struct Watch {
    looks: usize,
    seen_anything: bool,
    seen_main: bool,
    main_looks_in_a_row: usize,
}

impl Watch {
    /// Take one more look. `Some` once there is a verdict: `Ok` for started, or
    /// how it failed to.
    fn look(&mut self, sighting: Sighting) -> Option<Result<(), Bypass>> {
        self.looks += 1;
        if sighting.main {
            self.seen_anything = true;
            self.seen_main = true;
            self.main_looks_in_a_row += 1;
            return (self.main_looks_in_a_row >= STEADY_LOOKS).then_some(Ok(()));
        }
        self.main_looks_in_a_row = 0;
        if self.seen_main {
            return Some(Err(Bypass::ExitedEarly));
        }
        if sighting.any {
            self.seen_anything = true;
        } else if self.seen_anything {
            // The shim was there and is gone, and the app never came up.
            return Some(Err(Bypass::ExitedEarly));
        }
        if self.seen_anything {
            (self.looks >= LAUNCH_LOOKS).then_some(Ok(()))
        } else {
            (self.looks >= APPEAR_LOOKS).then_some(Err(Bypass::NeverStarted))
        }
    }
}

/// Keep looking, `pause` apart, until [`Watch`] has a verdict.
fn watch_start(pause: Duration, mut look: impl FnMut() -> Sighting) -> Result<(), Bypass> {
    let mut watch = Watch::default();
    loop {
        thread::sleep(pause);
        if let Some(verdict) = watch.look(look()) {
            return verdict;
        }
    }
}

/// Pure: whether any process in `ps_output` is running from inside `bundle`.
fn runs_from(ps_output: &str, bundle: &Path) -> bool {
    let inside = format!("{}/Contents/", bundle.display());
    ps_output.lines().any(|line| line.contains(&inside))
}

/// Look at the running processes for `bundle`'s wrapper on `data_dir`. If the
/// list can't be read there is no telling, and a second instance on the same data
/// would be worse than trusting the launch, so it counts as running.
fn sighting(data_dir: &str, gui_macos_exec: &str, bundle: &Path) -> Sighting {
    match process_list() {
        Ok(processes) => Sighting {
            main: find_running_pid(&processes, data_dir, gui_macos_exec).is_some(),
            any: runs_from(&processes, bundle),
        },
        Err(_) => Sighting {
            main: true,
            any: true,
        },
    }
}

/// What a launch does to the system, kept apart from the decisions around it so
/// those can be tested without starting anything.
trait Effects {
    /// Where the profile's wrapper stands against the installed vendor app.
    fn wrapper_state(&self) -> WrapperState;
    /// Open the profile's script launcher.
    fn open_script_launcher(&mut self) -> AppResult<()>;
    /// Build the wrapper again from the installed vendor app.
    fn rebuild_wrapper(&mut self) -> AppResult<()>;
    /// Open the wrapper and see that it stays up.
    fn open_wrapper(&mut self) -> Result<(), Bypass>;
    /// Start the vendor app on the profile's data, without the wrapper.
    fn open_stock(&mut self) -> AppResult<()>;
}

/// Pure: what is still wrong with a wrapper that has just been rebuilt, or
/// `None` if the rebuild produced what it was supposed to.
///
/// A rebuild that reports success without leaving a current wrapper must not be
/// opened anyway. The wrapper itself asks for the rebuild when it finds it has
/// fallen behind the vendor app, so opening one that is still behind would have
/// it ask again, and again.
fn rebuild_did_not_take(state: WrapperState) -> Option<&'static str> {
    match state {
        WrapperState::Current => None,
        WrapperState::Missing => Some("it is not there afterwards"),
        WrapperState::Stale => Some("it still does not match the installed app"),
    }
}

/// Start a profile's app: through its wrapper if it has one, through the stock
/// app if that does not work out. Returns why the wrapper was bypassed, if it
/// was. Fails only if the stock app cannot be started either.
fn launch_with<E: Effects>(distinct_dock_icon: bool, effects: &mut E) -> AppResult<Option<Bypass>> {
    let wrapped = match route(distinct_dock_icon, effects.wrapper_state()) {
        Route::ScriptLauncher => return effects.open_script_launcher().map(|()| None),
        Route::Wrapper => effects.open_wrapper(),
        Route::RebuildWrapper => match effects.rebuild_wrapper() {
            Ok(()) => match rebuild_did_not_take(effects.wrapper_state()) {
                None => effects.open_wrapper(),
                Some(problem) => Err(Bypass::RebuildFailed(format!(
                    "the wrapper was rebuilt but {problem}"
                ))),
            },
            Err(err) => Err(Bypass::RebuildFailed(err.message())),
        },
    };
    match wrapped {
        Ok(()) => Ok(None),
        Err(bypass) => {
            effects.open_stock().map_err(|err| {
                AppError::Validation(format!(
                    "{bypass} Opening the stock app failed too: {}",
                    err.message()
                ))
            })?;
            Ok(Some(bypass))
        }
    }
}

/// The real thing: one profile's launch, against the actual machine.
struct ProfileLaunch<'a> {
    profile: &'a Profile,
    version: &'a str,
    spec: &'static AppSpec,
    /// The stock app the profile is a variant of; `None` if it isn't installed.
    vendor: Option<ResolvedGuiApp>,
    /// `/Applications/<App> (<Name>).app`, whichever shape it has.
    launcher: PathBuf,
    data_dir: &'a str,
}

impl Effects for ProfileLaunch<'_> {
    fn wrapper_state(&self) -> WrapperState {
        let vendor = self
            .vendor
            .as_ref()
            .map(|vendor| vendor.bundle_path.as_path());
        wrapper::state(vendor, &self.launcher, self.version)
    }

    fn open_script_launcher(&mut self) -> AppResult<()> {
        open_bundle(&self.launcher)?;
        // The launcher only shells out again, so `open` returning says nothing
        // about the app being up. Without this the caller is told the launch is
        // over before there is a window, and a profile with a script launcher
        // would flicker where one with a wrapper reports honestly.
        let Some(vendor) = &self.vendor else {
            return Ok(());
        };
        let (data_dir, exec) = (self.data_dir, vendor.macos_exec);
        wait_until_up(LOOK_INTERVAL, SETTLE_LOOKS, || seems_up(data_dir, exec));
        Ok(())
    }

    fn rebuild_wrapper(&mut self) -> AppResult<()> {
        gui::generate(self.profile, self.version).map(|_| ())
    }

    fn open_wrapper(&mut self) -> Result<(), Bypass> {
        open_bundle(&self.launcher).map_err(|err| Bypass::OpenFailed(err.message()))?;
        let Some(vendor) = &self.vendor else {
            // Its process can't be told without the vendor's executable name.
            return Ok(());
        };
        watch_start(LOOK_INTERVAL, || {
            sighting(self.data_dir, vendor.macos_exec, &self.launcher)
        })
    }

    fn open_stock(&mut self) -> AppResult<()> {
        let config_home = cli_config_dir(&self.profile.id)?;
        open_new_instance(
            self.data_dir,
            self.spec,
            Some((self.spec.cli_config_env, config_home.as_path())),
        )
    }
}

/// Open `profile`'s desktop app, or focus it if it is already running, and say
/// why its wrapper was bypassed if it had to be.
///
/// A running instance is left alone, wrapper or not: a wrapper is the running
/// app's own bundle, so rebuilding it underneath would pull the files out from
/// under it. `version` is this app's own, for a wrapper that does get rebuilt.
/// `focus` brings the running instance forward, as for [`focus_or_launch`].
///
/// Waits for the app to come up, which can take a minute in the worst case, so
/// this does not belong on the main thread.
pub fn open_profile(
    profile: &Profile,
    version: &str,
    focus: impl FnOnce(i32),
) -> AppResult<Option<Bypass>> {
    let spec = profile.app.spec();
    let data_dir = profile_dir(&profile.id)?
        .join("gui-data")
        .display()
        .to_string();
    let mut effects = ProfileLaunch {
        profile,
        version,
        spec,
        vendor: resolve_gui_app(spec),
        launcher: gui_launcher_path(&profile.name, spec),
        data_dir: &data_dir,
    };
    let mut bypass = None;
    focus_or_launch(&data_dir, spec, focus, || {
        bypass = launch_with(profile.distinct_dock_icon, &mut effects)?;
        Ok(())
    })?;
    Ok(bypass)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn apps_are_started_without_a_config_home_ai_profiles_inherited() {
        let removed = |command: &Command, name: &str| {
            command
                .get_envs()
                .any(|(key, value)| key == name && value.is_none())
        };
        let stock = open_command();
        assert_eq!(stock.get_program(), "open");
        assert!(removed(&stock, "CLAUDE_CONFIG_DIR"));
        assert!(removed(&stock, "CODEX_HOME"));

        // A profile's own is set after, and wins.
        let mut profile = open_command();
        profile.env("CLAUDE_CONFIG_DIR", "/p/work/cli-config");
        assert!(profile
            .get_envs()
            .any(|(key, value)| key == "CLAUDE_CONFIG_DIR"
                && value == Some(std::ffi::OsStr::new("/p/work/cli-config"))));
    }

    const STOCK_DIR: &str = "/Users/me/Library/Application Support/Claude";
    const PROFILE_DIR: &str =
        "/Users/me/Library/Application Support/ai-profiles/profiles/abc/gui-data";

    /// Mirrors real `ps -ax -o pid=,command=` output: leading-padded PIDs, a
    /// main process per instance, and helper/crashpad lines that also carry
    /// `--user-data-dir` but must NOT match.
    fn sample_ps() -> String {
        format!(
            "\
  54587 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={STOCK_DIR}
  56318 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={PROFILE_DIR}
  54590 /Applications/Claude.app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper --type=gpu-process --user-data-dir={STOCK_DIR} --gpu-preferences=xyz
   1866 /Applications/Claude.app/Contents/Frameworks/Electron Framework.framework/Helpers/chrome_crashpad_handler --database={STOCK_DIR}/Crashpad --handshake-fd=18
"
        )
    }

    #[test]
    fn matches_main_process_for_the_stock_data_dir() {
        assert_eq!(
            find_running_pid(&sample_ps(), STOCK_DIR, "Claude"),
            Some(54587)
        );
    }

    #[test]
    fn matches_main_process_for_a_profile_data_dir() {
        assert_eq!(
            find_running_pid(&sample_ps(), PROFILE_DIR, "Claude"),
            Some(56318)
        );
    }

    #[test]
    fn returns_none_when_no_instance_is_bound_to_the_dir() {
        let other = "/Users/me/Library/Application Support/ai-profiles/profiles/zzz/gui-data";
        assert_eq!(find_running_pid(&sample_ps(), other, "Claude"), None);
    }

    #[test]
    fn ignores_helper_processes_that_share_the_data_dir() {
        // A renderer/GPU helper carries --user-data-dir but is not the main
        // process; if only helpers were running we must report nothing.
        let helpers_only = format!(
            "  54590 /Applications/Claude.app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper --type=renderer --user-data-dir={STOCK_DIR} --enable-sandbox\n"
        );
        assert_eq!(find_running_pid(&helpers_only, STOCK_DIR, "Claude"), None);
    }

    #[test]
    fn does_not_let_the_stock_dir_match_a_profile_whose_path_extends_it() {
        // The stock dir is a path prefix of nothing here, but guard the
        // reverse: searching the stock dir must not match the longer profile
        // line, and vice versa.
        assert_ne!(
            find_running_pid(&sample_ps(), STOCK_DIR, "Claude"),
            Some(56318)
        );
        assert_ne!(
            find_running_pid(&sample_ps(), PROFILE_DIR, "Claude"),
            Some(54587)
        );
    }

    #[test]
    fn matches_per_app_exec_name_only() {
        let codex_dir = "/Users/me/Library/Application Support/Codex";
        let codex_ps = format!(
            "  77001 /Applications/Codex.app/Contents/MacOS/Codex --user-data-dir={codex_dir}\n"
        );
        // The Codex exec matches under "Codex" but is invisible under "Claude".
        assert_eq!(find_running_pid(&codex_ps, codex_dir, "Codex"), Some(77001));
        assert_eq!(find_running_pid(&codex_ps, codex_dir, "Claude"), None);
    }

    /// What `ps` shows for a profile with a wrapper: the vendor binary the shim
    /// started, inside the wrapper, and the helpers under the wrapper's own
    /// Frameworks.
    fn wrapped_ps() -> String {
        format!(
            "\
  61234 /Applications/Claude (Work).app/Contents/MacOS/Claude.bin --user-data-dir={PROFILE_DIR}
  61240 /Applications/Claude (Work).app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper --type=gpu-process --user-data-dir={PROFILE_DIR} --gpu-preferences=xyz
"
        )
    }

    #[test]
    fn matches_the_main_process_of_a_wrapper() {
        assert_eq!(
            find_running_pid(&wrapped_ps(), PROFILE_DIR, "Claude"),
            Some(61234)
        );
    }

    #[test]
    fn a_wrapper_is_no_instance_of_another_data_dir_or_another_app() {
        assert_eq!(find_running_pid(&wrapped_ps(), STOCK_DIR, "Claude"), None);
        assert_eq!(
            find_running_pid(&wrapped_ps(), PROFILE_DIR, "ChatGPT"),
            None
        );
    }

    #[test]
    fn a_wrapper_and_the_stock_app_running_side_by_side_are_told_apart_by_data_dir() {
        let side_by_side = format!(
            "  54587 /Applications/Claude.app/Contents/MacOS/Claude --user-data-dir={STOCK_DIR}\n{}",
            wrapped_ps()
        );
        assert_eq!(
            find_running_pid(&side_by_side, STOCK_DIR, "Claude"),
            Some(54587)
        );
        assert_eq!(
            find_running_pid(&side_by_side, PROFILE_DIR, "Claude"),
            Some(61234)
        );
    }

    #[test]
    fn the_wrapper_search_finds_wrappers_and_nothing_else() {
        assert_eq!(
            find_running_wrapper_pid(&wrapped_ps(), PROFILE_DIR, "Claude"),
            Some(61234)
        );
        // The stock app bound to the same dir is not a wrapper.
        assert_eq!(
            find_running_wrapper_pid(&sample_ps(), PROFILE_DIR, "Claude"),
            None
        );
    }

    #[test]
    fn a_profile_without_a_wrapper_is_never_running_from_one() {
        let mut profile = wrapped_profile();
        profile.distinct_dock_icon = false;
        assert_eq!(running_wrapper(&profile).unwrap(), None);
    }

    #[test]
    fn a_running_wrapper_is_found_and_a_stopped_one_is_not() {
        let root = tempfile::tempdir().unwrap();
        let profile = wrapped_profile();
        let data_dir = profile_dir(&profile.id).unwrap().join("gui-data");
        assert_eq!(running_wrapper(&profile).unwrap(), None);

        let mut process = crate::test_support::fake_wrapper_process(root.path(), &data_dir);
        let pid = i32::try_from(process.id()).unwrap();
        let found = running_wrapper(&profile);
        process.kill().unwrap();
        process.wait().unwrap();

        assert_eq!(found.unwrap(), Some(pid));
        assert_eq!(running_wrapper(&profile).unwrap(), None);
    }

    #[test]
    fn the_launcher_is_chosen_by_the_setting_and_by_where_the_wrapper_stands() {
        use WrapperState::{Current, Missing, Stale};

        for state in [Missing, Stale, Current] {
            assert_eq!(route(false, state), Route::ScriptLauncher, "{state:?}");
        }
        assert_eq!(route(true, Current), Route::Wrapper);
        assert_eq!(route(true, Stale), Route::RebuildWrapper);
        assert_eq!(route(true, Missing), Route::RebuildWrapper);
    }

    const NOTHING: Sighting = Sighting {
        main: false,
        any: false,
    };
    /// Only the shim: started, and not yet the app.
    const SHIM: Sighting = Sighting {
        main: false,
        any: true,
    };
    const APP: Sighting = Sighting {
        main: true,
        any: true,
    };

    /// `count` looks that all turn up `sighting`.
    fn looks(sighting: Sighting, count: usize) -> Vec<Sighting> {
        vec![sighting; count]
    }

    /// Replays `script` as the sightings, holding on the last one once it runs
    /// out, and counts how many looks it took to reach a verdict.
    fn watch(script: &[Sighting]) -> (Result<(), Bypass>, usize) {
        let mut asked = 0;
        let verdict = watch_start(Duration::ZERO, || {
            let sighting = script[asked.min(script.len() - 1)];
            asked += 1;
            sighting
        });
        (verdict, asked)
    }

    #[test]
    fn an_app_that_is_up_and_stays_up_started() {
        assert_eq!(watch(&[APP]), (Ok(()), STEADY_LOOKS));
    }

    #[test]
    fn an_app_that_takes_a_moment_to_come_up_started() {
        let mut script = looks(NOTHING, 10);
        script.extend(looks(SHIM, 5));
        script.push(APP);
        assert_eq!(watch(&script), (Ok(()), 15 + STEADY_LOOKS));
    }

    #[test]
    fn an_app_that_macos_holds_for_seconds_before_it_starts_is_waited_for() {
        // The first run of a bundle macOS has not seen: the shim is there for a
        // long time, and then the app. Starting the stock app meanwhile would
        // give the profile two instances.
        let mut script = looks(SHIM, 300);
        script.push(APP);
        assert_eq!(watch(&script), (Ok(()), 300 + STEADY_LOOKS));
    }

    #[test]
    fn nothing_that_ever_shows_up_never_started() {
        assert_eq!(watch(&[NOTHING]), (Err(Bypass::NeverStarted), APPEAR_LOOKS));
    }

    #[test]
    fn something_that_shows_up_only_just_in_time_is_still_waited_for() {
        let mut script = looks(NOTHING, APPEAR_LOOKS - 1);
        script.push(SHIM);
        script.push(APP);
        assert_eq!(watch(&script), (Ok(()), APPEAR_LOOKS + STEADY_LOOKS));
    }

    #[test]
    fn a_shim_that_goes_away_without_the_app_coming_up_failed() {
        assert_eq!(watch(&[SHIM, SHIM, NOTHING]), (Err(Bypass::ExitedEarly), 3));
    }

    #[test]
    fn an_app_that_goes_away_soon_after_it_came_up_failed() {
        let mut script = looks(SHIM, 2);
        script.extend(looks(APP, STEADY_LOOKS - 1));
        script.push(NOTHING);
        assert_eq!(watch(&script), (Err(Bypass::ExitedEarly), 2 + STEADY_LOOKS));
    }

    #[test]
    fn an_app_that_goes_away_with_its_helpers_still_about_failed_too() {
        assert_eq!(watch(&[APP, APP, SHIM]), (Err(Bypass::ExitedEarly), 3));
    }

    #[test]
    fn a_launch_that_is_still_going_after_a_minute_is_left_to_get_on_with_it() {
        assert_eq!(watch(&[SHIM]), (Ok(()), LAUNCH_LOOKS));
    }

    #[test]
    fn a_wrapper_that_takes_seconds_to_start_is_waited_for_on_the_real_process_list() {
        let root = tempfile::tempdir().unwrap();
        let data_dir = root.path().join("gui-data");
        let (mut process, bundle) =
            crate::test_support::slow_wrapper_process(root.path(), &data_dir, 2, true);

        let verdict = watch_start(Duration::from_millis(50), || {
            sighting(&data_dir.display().to_string(), "Claude", &bundle)
        });
        process.kill().unwrap();
        process.wait().unwrap();

        assert_eq!(verdict, Ok(()));
    }

    #[test]
    fn a_wrapper_whose_shim_exits_without_starting_the_app_is_seen_to_have_failed() {
        let root = tempfile::tempdir().unwrap();
        let data_dir = root.path().join("gui-data");
        let (mut process, bundle) =
            crate::test_support::slow_wrapper_process(root.path(), &data_dir, 1, false);

        let verdict = watch_start(Duration::from_millis(50), || {
            sighting(&data_dir.display().to_string(), "Claude", &bundle)
        });
        let _ = process.kill();
        process.wait().unwrap();

        assert_eq!(verdict, Err(Bypass::ExitedEarly));
    }

    #[test]
    fn processes_are_told_to_be_running_from_a_bundle_by_where_they_run_from() {
        let bundle = Path::new("/Applications/Claude (Work).app");
        let inside = "  61234 /Applications/Claude (Work).app/Contents/MacOS/Claude\n";
        let helper = "  61240 /Applications/Claude (Work).app/Contents/Frameworks/Claude Helper.app/Contents/MacOS/Claude Helper --type=gpu-process\n";
        assert!(runs_from(inside, bundle));
        assert!(runs_from(helper, bundle));

        let elsewhere = "  54587 /Applications/Claude.app/Contents/MacOS/Claude\n\
                         \x20 70001 /Applications/Claude (Play).app/Contents/MacOS/Claude\n";
        assert!(!runs_from(elsewhere, bundle));
        // Merely naming the bundle is not running from it.
        let opener = "  70002 open /Applications/Claude (Work).app\n";
        assert!(!runs_from(opener, bundle));
        assert!(!runs_from("", bundle));
    }

    #[test]
    fn a_cause_is_quoted_by_its_first_line_and_kept_short() {
        assert_eq!(brief("short"), "short");
        assert_eq!(brief("  first\nsecond"), "first");
        assert_eq!(brief(""), "");

        let long = "x".repeat(BRIEF_MAX_CHARS + 50);
        let cut = brief(&long);
        assert_eq!(cut.chars().count(), BRIEF_MAX_CHARS + 1);
        assert!(cut.ends_with('…'));
        // At the limit nothing is cut.
        let exact = "y".repeat(BRIEF_MAX_CHARS);
        assert_eq!(brief(&exact), exact);
    }

    /// Records what a launch asks for and answers from a script.
    struct Fake {
        state: WrapperState,
        /// Where a successful rebuild leaves the wrapper. `Current`, as a real
        /// one does, unless a test is about a rebuild that did not take.
        after_rebuild: WrapperState,
        rebuild_error: Option<AppError>,
        wrapper: Result<(), Bypass>,
        stock_error: Option<AppError>,
        calls: Vec<&'static str>,
    }

    impl Fake {
        fn new(state: WrapperState) -> Self {
            Fake {
                state,
                after_rebuild: WrapperState::Current,
                rebuild_error: None,
                wrapper: Ok(()),
                stock_error: None,
                calls: Vec::new(),
            }
        }
    }

    impl Effects for Fake {
        fn wrapper_state(&self) -> WrapperState {
            self.state
        }

        fn open_script_launcher(&mut self) -> AppResult<()> {
            self.calls.push("script launcher");
            Ok(())
        }

        fn rebuild_wrapper(&mut self) -> AppResult<()> {
            self.calls.push("rebuild");
            if let Some(err) = self.rebuild_error.take() {
                return Err(err);
            }
            self.state = self.after_rebuild;
            Ok(())
        }

        fn open_wrapper(&mut self) -> Result<(), Bypass> {
            self.calls.push("wrapper");
            self.wrapper.clone()
        }

        fn open_stock(&mut self) -> AppResult<()> {
            self.calls.push("stock");
            self.stock_error.take().map_or(Ok(()), Err)
        }
    }

    #[test]
    fn a_profile_without_a_wrapper_opens_its_script_launcher_and_nothing_else() {
        for state in [WrapperState::Missing, WrapperState::Current] {
            let mut fake = Fake::new(state);
            assert_eq!(launch_with(false, &mut fake).unwrap(), None);
            assert_eq!(fake.calls, ["script launcher"]);
        }
    }

    #[test]
    fn a_current_wrapper_is_opened_as_it_is() {
        let mut fake = Fake::new(WrapperState::Current);
        assert_eq!(launch_with(true, &mut fake).unwrap(), None);
        assert_eq!(fake.calls, ["wrapper"]);
    }

    #[test]
    fn a_missing_or_stale_wrapper_is_rebuilt_before_it_is_opened() {
        for state in [WrapperState::Missing, WrapperState::Stale] {
            let mut fake = Fake::new(state);
            assert_eq!(launch_with(true, &mut fake).unwrap(), None);
            assert_eq!(fake.calls, ["rebuild", "wrapper"], "{state:?}");
        }
    }

    #[test]
    fn a_failed_rebuild_starts_the_stock_app_without_trying_the_wrapper() {
        let mut fake = Fake::new(WrapperState::Stale);
        fake.rebuild_error = Some(AppError::Validation("disk is full".into()));

        let bypass = launch_with(true, &mut fake).unwrap();

        assert_eq!(fake.calls, ["rebuild", "stock"]);
        let Some(Bypass::RebuildFailed(detail)) = bypass else {
            panic!("expected a failed rebuild, got {bypass:?}");
        };
        assert!(detail.contains("disk is full"), "{detail}");
    }

    #[test]
    fn waiting_stops_as_soon_as_the_app_is_there() {
        let mut looks = 0;
        wait_until_up(Duration::ZERO, 100, || {
            looks += 1;
            looks == 3
        });

        assert_eq!(looks, 3);
    }

    #[test]
    fn waiting_gives_up_rather_than_holding_the_caller_for_ever() {
        let mut looks = 0;
        wait_until_up(Duration::ZERO, 5, || {
            looks += 1;
            false
        });

        assert_eq!(looks, 5);
    }

    #[test]
    fn nothing_is_waited_for_when_no_looks_are_allowed() {
        let mut looked = false;
        wait_until_up(Duration::ZERO, 0, || {
            looked = true;
            true
        });

        assert!(!looked);
    }

    #[test]
    fn only_a_current_wrapper_counts_as_a_rebuild_that_took() {
        assert_eq!(rebuild_did_not_take(WrapperState::Current), None);
        assert!(rebuild_did_not_take(WrapperState::Missing).is_some());
        assert!(rebuild_did_not_take(WrapperState::Stale).is_some());
    }

    #[test]
    fn a_rebuild_that_reports_success_but_leaves_the_wrapper_behind_is_not_opened() {
        for left in [WrapperState::Stale, WrapperState::Missing] {
            let mut fake = Fake::new(WrapperState::Stale);
            fake.after_rebuild = left;

            let bypass = launch_with(true, &mut fake).unwrap();

            assert_eq!(fake.calls, ["rebuild", "stock"], "{left:?}");
            let Some(Bypass::RebuildFailed(detail)) = bypass else {
                panic!("expected a failed rebuild, got {bypass:?}");
            };
            assert!(detail.contains("rebuilt but"), "{detail}");
        }
    }

    #[test]
    fn a_wrapper_that_will_not_stay_up_starts_the_stock_app_and_says_why() {
        for cause in [
            Bypass::OpenFailed("launch failed".into()),
            Bypass::NeverStarted,
            Bypass::ExitedEarly,
        ] {
            let mut fake = Fake::new(WrapperState::Current);
            fake.wrapper = Err(cause.clone());

            let bypass = launch_with(true, &mut fake).unwrap();

            assert_eq!(bypass, Some(cause));
            assert_eq!(fake.calls, ["wrapper", "stock"]);
        }
    }

    #[test]
    fn a_wrapper_rebuilt_and_then_broken_still_falls_back() {
        let mut fake = Fake::new(WrapperState::Missing);
        fake.wrapper = Err(Bypass::ExitedEarly);

        assert_eq!(
            launch_with(true, &mut fake).unwrap(),
            Some(Bypass::ExitedEarly)
        );
        assert_eq!(fake.calls, ["rebuild", "wrapper", "stock"]);
    }

    #[test]
    fn when_the_stock_app_will_not_start_either_the_error_carries_both_causes() {
        let mut fake = Fake::new(WrapperState::Current);
        fake.wrapper = Err(Bypass::NeverStarted);
        fake.stock_error = Some(AppError::Validation("Claude isn't installed".into()));

        let error = launch_with(true, &mut fake).unwrap_err().to_string();

        assert!(error.contains("Claude isn't installed"), "{error}");
        assert!(error.contains(&Bypass::NeverStarted.to_string()), "{error}");
    }

    /// The fixture of an opted-in profile, for the opt-in tests below.
    fn wrapped_profile() -> Profile {
        Profile {
            id: "feedface-0000-0000-0000-000000000006".into(),
            app: crate::app_kind::AppKind::Claude,
            name: "PhaseSixTest".into(),
            slug: "phasesixtest".into(),
            color: "#7C3AED".into(),
            created_at: "2026-05-20T12:00:00Z".into(),
            surfaces: crate::profiles::Surfaces {
                gui: true,
                cli: false,
            },
            distinct_dock_icon: true,
            last_used_at: None,
        }
    }

    /// `true` once `condition` holds, checked every 200 ms for up to ten seconds.
    fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
        for _ in 0..50 {
            if condition() {
                return true;
            }
            thread::sleep(Duration::from_millis(200));
        }
        false
    }

    fn stop(pid: i32) {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }

    /// Stops whatever the profile has running and removes its launcher, however
    /// the test ends.
    struct Cleanup<'a> {
        profile: &'a Profile,
        data_dir: &'a str,
        exec: &'static str,
    }

    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            if let Ok(Some(pid)) = running_pid(self.data_dir, self.exec) {
                stop(pid);
                wait_until(|| matches!(running_pid(self.data_dir, self.exec), Ok(None)));
            }
            let _ = gui::remove(&self.profile.name, self.profile.app.spec());
        }
    }

    /// Opt-in: builds a real wrapper from the installed Claude under
    /// /Applications and opens it, then breaks it and opens again to see the
    /// stock app take over. Starts real Claude processes on a throwaway data
    /// dir, so it is gated behind AI_PROFILES_E2E=1.
    #[test]
    fn a_wrapped_profile_opens_through_its_wrapper_and_a_broken_wrapper_falls_back() {
        if std::env::var("AI_PROFILES_E2E").is_err() {
            eprintln!("skipping; set AI_PROFILES_E2E=1 to run");
            return;
        }
        let profile = wrapped_profile();
        let spec = profile.app.spec();
        let Some(vendor) = resolve_gui_app(spec) else {
            eprintln!("Claude not installed; skipping");
            return;
        };
        let data_dir = profile_dir(&profile.id)
            .unwrap()
            .join("gui-data")
            .display()
            .to_string();
        let bundle = gui_launcher_path(&profile.name, spec);
        let _cleanup = Cleanup {
            profile: &profile,
            data_dir: &data_dir,
            exec: vendor.macos_exec,
        };
        let running = || running_pid(&data_dir, vendor.macos_exec).unwrap();
        let wrapper_binary = format!("{}/Contents/MacOS/Claude.bin", bundle.display());

        // Nothing there yet: it is built, then opened through the wrapper.
        assert_eq!(open_profile(&profile, "0.1.0", focus_pid).unwrap(), None);
        let first = running().expect("the wrapper is running");
        assert!(process_list().unwrap().contains(&wrapper_binary));

        // Opening it again focuses that instance and starts no other.
        assert_eq!(open_profile(&profile, "0.1.0", focus_pid).unwrap(), None);
        assert_eq!(running(), Some(first));

        stop(first);
        assert!(wait_until(|| running().is_none()), "the wrapper stopped");

        // With the vendor binary gone the shim has nothing to start.
        fs::remove_file(&wrapper_binary).unwrap();
        let bypass = open_profile(&profile, "0.1.0", focus_pid).unwrap();
        assert!(
            matches!(bypass, Some(Bypass::NeverStarted | Bypass::ExitedEarly)),
            "{bypass:?}"
        );
        assert!(
            wait_until(|| running().is_some()),
            "the stock app took over"
        );
        assert!(!process_list().unwrap().contains(&wrapper_binary));
    }
}

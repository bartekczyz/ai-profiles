//! Shared test fixtures.
//!
//! `app_data_dir()` resolves to a single filesystem path that every module
//! shares. The `app_state` and `profiles` tests each `remove_dir_all` it
//! before/after their assertions, so they must serialize against each
//! other — a per-module mutex would still let cross-module tests race.
//! This module hosts the global mutex they all lock.

#![cfg(test)]

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use serde_json::Value;

use crate::app_kind::AppKind;
use crate::sessions::Home;

pub(crate) static APP_DIR_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Starts a process that `ps` shows as the vendor binary of a wrapper running on
/// `data_dir`: `<root>/Claude (Fake).app/Contents/MacOS/Claude.bin
/// --user-data-dir=<data_dir>`. It is a shell script that waits for input, which
/// its parent's end of the pipe never sends, so it runs until it is killed.
/// Returns once `ps` shows it so (see [`ready`]).
///
/// A real wrapper needs the vendor app installed and a link step; this needs
/// neither, and looks the same to the code that looks for it.
pub(crate) fn fake_wrapper_process(root: &Path, data_dir: &Path) -> Child {
    let child = wrapper_stand_in(
        root,
        data_dir,
        "#!/bin/sh\necho ready\nread _\n",
        Stdio::piped(),
    );
    ready(child, data_dir)
}

/// As [`fake_wrapper_process`], but it ignores SIGTERM, like an app that
/// won't quit when asked: only SIGKILL ends it. Returns once it ignores it
/// and `ps` shows it.
pub(crate) fn stubborn_wrapper_process(root: &Path, data_dir: &Path) -> Child {
    let child = wrapper_stand_in(
        root,
        data_dir,
        "#!/bin/sh\ntrap '' TERM\necho ready\nread _\n",
        Stdio::piped(),
    );
    ready(child, data_dir)
}

/// `child`, a stand-in wrapper on `data_dir` whose script says `ready` once it
/// runs, when it is ready to be looked for: it said so, and `ps` lists it with
/// its whole command line. Starting a script execs a shell, which on macOS
/// execs another, and until the last has started `ps` can show the process by
/// another command line, or by its name alone.
fn ready(mut child: Child, data_dir: &Path) -> Child {
    let mut said = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut said)
        .unwrap();
    assert_eq!(said.trim(), "ready");
    let pid = child.id().to_string();
    let command = format!(
        "/Contents/MacOS/Claude.bin --user-data-dir={}",
        data_dir.display()
    );
    for _ in 0..400 {
        let listed = crate::launch::process_list().unwrap();
        let shown = listed.lines().any(|line| {
            line.trim_start()
                .split_once(char::is_whitespace)
                .is_some_and(|(listed_pid, listed_command)| {
                    listed_pid == pid && listed_command.trim_end().ends_with(&command)
                })
        });
        if shown {
            return child;
        }
        thread::sleep(Duration::from_millis(25));
    }
    panic!("ps never showed the stand-in wrapper as {command}");
}

/// Starts `script` as `<root>/Claude (Fake).app/Contents/MacOS/Claude.bin
/// --user-data-dir=<data_dir>`, its output going to `stdout`.
fn wrapper_stand_in(root: &Path, data_dir: &Path, script: &str, stdout: Stdio) -> Child {
    let macos = root.join("Claude (Fake).app/Contents/MacOS");
    fs::create_dir_all(&macos).unwrap();
    let binary = macos.join("Claude.bin");
    write_executable(&binary, script);
    let mut stdout = Some(stdout);

    // Running a file just written can meet "text file busy" while another test
    // thread is forking, which is over as soon as that fork has exec'd.
    for _ in 0..40 {
        match Command::new(&binary)
            .arg(format!("--user-data-dir={}", data_dir.display()))
            .stdin(Stdio::piped())
            .stdout(stdout.take().unwrap_or_else(Stdio::piped))
            .spawn()
        {
            Ok(child) => return child,
            Err(err) if err.raw_os_error() == Some(TEXT_FILE_BUSY) => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(err) => panic!("could not start the stand-in wrapper: {err}"),
        }
    }
    panic!("the stand-in wrapper stayed busy");
}

/// Starts what looks, to `ps`, like a wrapper that macOS is slow to start.
///
/// For `hold_seconds` the process is only the shim,
/// `<root>/Claude (Slow).app/Contents/MacOS/Claude`, with no arguments, as
/// LaunchServices starts the real one. Then, if `starts`, it becomes the vendor
/// binary `Claude.bin --user-data-dir=<data_dir>` beside it, and otherwise it
/// exits. Returns the process and the bundle it runs from.
pub(crate) fn slow_wrapper_process(
    root: &Path,
    data_dir: &Path,
    hold_seconds: u32,
    starts: bool,
) -> (Child, PathBuf) {
    let bundle = root.join("Claude (Slow).app");
    let macos = bundle.join("Contents/MacOS");
    fs::create_dir_all(&macos).unwrap();
    let after_hold = if starts {
        "exec \"$(dirname \"$0\")/Claude.bin\" --user-data-dir=\"$FAKE_DATA_DIR\"\n"
    } else {
        "exit 1\n"
    };
    write_executable(
        &macos.join("Claude"),
        &format!("#!/bin/sh\nsleep {hold_seconds}\n{after_hold}"),
    );
    write_executable(&macos.join("Claude.bin"), "#!/bin/sh\nread _\n");

    let shim = macos.join("Claude");
    for _ in 0..40 {
        match Command::new(&shim)
            .env("FAKE_DATA_DIR", data_dir)
            .stdin(Stdio::piped())
            .spawn()
        {
            Ok(child) => return (child, bundle),
            Err(err) if err.raw_os_error() == Some(TEXT_FILE_BUSY) => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(err) => panic!("could not start the stand-in wrapper: {err}"),
        }
    }
    panic!("the stand-in wrapper stayed busy");
}

fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// What `spawn` reports when the file being run has only just been written and
/// another test thread was forking at the time.
const TEXT_FILE_BUSY: i32 = 26;

/// Every file under `root`, with its contents. Empty folders don't show.
pub(crate) fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                pending.push(path);
            } else {
                files.insert(path.clone(), fs::read(&path).unwrap());
            }
        }
    }
    files
}

/// A managed Claude home named `name` under `root`: its config dir at
/// `<root>/<name>/cli-config` and its desktop app's data at
/// `<root>/<name>/gui-data`, neither made.
pub(crate) fn claude_home(root: &Path, name: &str) -> Home {
    Home {
        id: name.to_string(),
        app: AppKind::Claude,
        label: name.to_string(),
        config_dir: root.join(name).join("cli-config"),
        gui_data_dir: root.join(name).join("gui-data"),
        stock: false,
        desktop_reads_config_dir: true,
    }
}

/// [`claude_home`], whose desktop app has been opened, so its data dir is
/// there.
pub(crate) fn opened_claude_home(root: &Path, name: &str) -> Home {
    let home = claude_home(root, name);
    fs::create_dir_all(&home.gui_data_dir).unwrap();
    home
}

/// The JSON file at `path`.
pub(crate) fn read_value(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// Reap `child` on another thread once it exits, as launchd reaps a real
/// app, so it leaves the process list. Its stdin is kept open and returned,
/// as a stand-in exits when that closes.
pub(crate) fn reaped(mut child: Child) -> (JoinHandle<bool>, Option<ChildStdin>) {
    let stdin = child.stdin.take();
    (thread::spawn(move || child.wait().is_ok()), stdin)
}

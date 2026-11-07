// SPDX-License-Identifier: MIT

//! The Developer menu of debug builds, for taking the README and landing-page
//! screenshots with fake data: it switches the frontend's demo data on, sets
//! the window's theme and size for the shot, and captures it. Release builds
//! leave this module out.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{mpsc, Mutex};

use objc2_app_kit::NSWindow;
use objc2_foundation::NSRect;
use serde::{Deserialize, Serialize};
use tauri::menu::{CheckMenuItem, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::plugin::TauriPlugin;
use tauri::{
    AppHandle, Builder, Emitter, LogicalSize, Manager, RunEvent, Runtime, Window, WindowEvent,
};

const DEMO_DATA_ID: &str = "dev-demo-data";
const LIGHT_ID: &str = "dev-theme-light";
const DARK_ID: &str = "dev-theme-dark";
const SCREENSHOT_SIZE_ID: &str = "dev-screenshot-size";
const CAPTURE_ID: &str = "dev-capture-screenshots";

/// The window's size in the screenshots, in points: 2560×1600 pixels on a
/// Retina display, before the shadow.
const SCREENSHOT_SIZE: LogicalSize<f64> = LogicalSize {
    width: 1280.0,
    height: 800.0,
};

/// The window's frame from before it was given the screenshot size, as
/// AppKit has it. Put back through AppKit, it takes effect at once, even as
/// the app quits; Tauri's setters only queue the change, which by then never
/// happens.
static SAVED_FRAME: Mutex<Option<NSRect>> = Mutex::new(None);

/// A theme the window can be put in.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DevTheme {
    Light,
    Dark,
}

impl DevTheme {
    fn name(self) -> &'static str {
        match self {
            DevTheme::Light => "light",
            DevTheme::Dark => "dark",
        }
    }
}

impl From<DevTheme> for tauri::Theme {
    fn from(theme: DevTheme) -> Self {
        match theme {
            DevTheme::Light => tauri::Theme::Light,
            DevTheme::Dark => tauri::Theme::Dark,
        }
    }
}

/// The Developer menu's items whose state changes.
struct DeveloperMenu<R: Runtime> {
    /// Demo Data, checked while it is on.
    demo_data: CheckMenuItem<R>,
    /// The items that only mean something while demo data is on.
    demo_only: Vec<MenuItem<R>>,
}

/// Adds what the Developer menu needs to `builder`: its menu handler, and a
/// plugin that puts a screenshot-sized window back as it closes or the app
/// quits. Goes ahead of the window-state plugin, so that one saves the window
/// as it was.
pub fn install<R: Runtime>(builder: Builder<R>) -> Builder<R> {
    builder
        .plugin(restore_frame_on_exit())
        .on_menu_event(on_menu_event)
}

fn restore_frame_on_exit<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::new("dev-tools")
        .on_window_ready(|window| {
            let target = window.clone();
            window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { .. } = event {
                    restore_frame(&target);
                }
            });
        })
        .on_event(|app, event| {
            // Quitting from the menu skips `ExitRequested`.
            if let RunEvent::ExitRequested { .. } | RunEvent::Exit = event {
                for window in app.webview_windows().values() {
                    restore_frame(&window.as_ref().window());
                }
            }
        })
        .build()
}

/// The Developer menu: demo data on or off, and while it is on, the theme
/// and the screenshot size; and the screenshots, taken in one go.
pub fn menu<R: Runtime, M: Manager<R>>(manager: &M) -> tauri::Result<Submenu<R>> {
    let demo_data = CheckMenuItem::with_id(
        manager,
        DEMO_DATA_ID,
        "Demo Data",
        true,
        false,
        None::<&str>,
    )?;
    let demo_only = vec![
        MenuItem::with_id(manager, LIGHT_ID, "Light", false, None::<&str>)?,
        MenuItem::with_id(manager, DARK_ID, "Dark", false, None::<&str>)?,
        MenuItem::with_id(
            manager,
            SCREENSHOT_SIZE_ID,
            "Screenshot Size",
            false,
            None::<&str>,
        )?,
    ];
    let submenu = Submenu::with_items(
        manager,
        "Developer",
        true,
        &[
            &demo_data,
            &demo_only[0],
            &demo_only[1],
            &demo_only[2],
            &PredefinedMenuItem::separator(manager)?,
            &MenuItem::with_id(
                manager,
                CAPTURE_ID,
                "Capture Screenshots",
                true,
                None::<&str>,
            )?,
        ],
    )?;
    manager.manage(DeveloperMenu {
        demo_data,
        demo_only,
    });
    Ok(submenu)
}

/// Passes a Developer menu choice on to the frontend, which does the work.
fn on_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let Some(menu) = app.try_state::<DeveloperMenu<R>>() else {
        return;
    };
    let _ = match event.id().as_ref() {
        DEMO_DATA_ID => {
            let on = menu.demo_data.is_checked().unwrap_or(false);
            for item in &menu.demo_only {
                let _ = item.set_enabled(on);
            }
            app.emit("dev-demo-data", on)
        }
        LIGHT_ID => app.emit("dev-theme", DevTheme::Light),
        DARK_ID => app.emit("dev-theme", DevTheme::Dark),
        SCREENSHOT_SIZE_ID => app.emit("dev-screenshot-size", ()),
        CAPTURE_ID => app.emit("dev-capture-screenshots", ()),
        _ => Ok(()),
    };
}

/// Puts the window's native parts (title bar, sidebar material) in `theme`,
/// or back to following the system with `None`.
#[tauri::command]
pub fn dev_set_window_theme<R: Runtime>(
    window: Window<R>,
    theme: Option<DevTheme>,
) -> Result<(), String> {
    window
        .set_theme(theme.map(Into::into))
        .map_err(|error| error.to_string())
}

/// Gives the window the screenshot size, centred and in front, remembering
/// where it was unless it already is screenshot-sized.
#[tauri::command]
pub fn dev_prepare_screenshot_window<R: Runtime>(window: Window<R>) -> Result<(), String> {
    let mut saved = SAVED_FRAME.lock().map_err(|error| error.to_string())?;
    if saved.is_none() {
        *saved = Some(with_ns_window(&window, |ns_window| ns_window.frame())?);
    }
    // In front, so the shot has the active window's title bar and material.
    window
        .set_size(SCREENSHOT_SIZE)
        .and_then(|()| window.center())
        .and_then(|()| window.set_focus())
        .map_err(|error| error.to_string())
}

/// Puts the window back where `dev_prepare_screenshot_window` found it.
#[tauri::command]
pub fn dev_restore_window<R: Runtime>(window: Window<R>) {
    restore_frame(&window);
}

/// Puts the window back where it was before it was screenshot-sized, if it
/// was. Only on the main thread.
fn restore_frame<R: Runtime>(window: &Window<R>) {
    let Some(frame) = SAVED_FRAME.lock().ok().and_then(|mut saved| saved.take()) else {
        return;
    };
    let _ = with_ns_window(window, |ns_window| ns_window.setFrame_display(frame, true));
}

/// Runs `action` on the window's `NSWindow`. Only on the main thread, as the
/// commands that aren't `async` and the window and app events run.
fn with_ns_window<R: Runtime, T>(
    window: &Window<R>,
    action: impl FnOnce(&NSWindow) -> T,
) -> Result<T, String> {
    let ns_window = window.ns_window().map_err(|error| error.to_string())?;
    // SAFETY: Tauri hands out the live `NSWindow` behind this window, and the
    // caller is on the main thread.
    Ok(action(unsafe { &*ns_window.cast::<NSWindow>() }))
}

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    /// Whether this process may record the screen. Asks nothing of the user.
    fn CGPreflightScreenCaptureAccess() -> bool;
}

/// Saves the window, shadow included, as the README's and landing page's
/// screenshot in `theme`, and returns where.
#[tauri::command]
pub async fn dev_capture_window<R: Runtime>(
    window: Window<R>,
    theme: DevTheme,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || capture_window(&window, theme))
        .await
        .map_err(|error| error.to_string())?
}

fn capture_window<R: Runtime>(window: &Window<R>, theme: DevTheme) -> Result<String, String> {
    // SAFETY: only reads this process's permission.
    if !unsafe { CGPreflightScreenCaptureAccess() } {
        return Err(
            "Screen Recording is off for the app that launched `tauri dev` (your terminal or IDE). \
            Turn it on in System Settings › Privacy & Security › Screen Recording, \
            then quit and reopen that app and run `tauri dev` again."
                .to_string(),
        );
    }
    let window_number = on_main_thread(window, |ns_window| ns_window.windowNumber())?;
    let path = screenshot_path(Path::new(env!("CARGO_MANIFEST_DIR")), theme);
    // Saved beside it first, so a failed capture leaves the last one as it was.
    let partial = path.with_extension("partial.png");
    let status = Command::new("/usr/sbin/screencapture")
        .args(screencapture_args(window_number, &partial))
        .status()
        .map_err(|error| format!("couldn't run screencapture: {error}"))?;
    let saved = status.success() && fs::metadata(&partial).is_ok_and(|file| file.len() > 0);
    if !saved {
        let _ = fs::remove_file(&partial);
        return Err(format!("screencapture saved nothing ({status})"));
    }
    fs::rename(&partial, &path).map_err(|error| error.to_string())?;
    Ok(fs::canonicalize(&path)
        .unwrap_or(path)
        .display()
        .to_string())
}

/// Pure: the screenshot in `theme`, among the landing page's public files,
/// which the README links to as well. `manifest_dir` is this crate's.
fn screenshot_path(manifest_dir: &Path, theme: DevTheme) -> PathBuf {
    manifest_dir
        .join("../../landing/public")
        .join(format!("screenshot-{}.png", theme.name()))
}

/// Pure: the `screencapture` arguments that save window `window_number`, with
/// its shadow and without the shutter sound, as a PNG at `path`.
fn screencapture_args(window_number: isize, path: &Path) -> Vec<OsString> {
    vec![
        "-x".into(),
        "-l".into(),
        window_number.to_string().into(),
        "-t".into(),
        "png".into(),
        path.into(),
    ]
}

/// Runs `action` on the window's `NSWindow` on the main thread, from another
/// thread, and waits for its answer.
fn on_main_thread<R: Runtime, T: Send + 'static>(
    window: &Window<R>,
    action: impl FnOnce(&NSWindow) -> T + Send + 'static,
) -> Result<T, String> {
    let (sender, receiver) = mpsc::channel();
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            let _ = sender.send(with_ns_window(&target, action));
        })
        .map_err(|error| error.to_string())?;
    receiver.recv().map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_path_is_the_landing_pages_public_file_for_the_theme() {
        let manifest_dir = Path::new("/repo/apps/ai-profiles/src-tauri");

        let path = screenshot_path(manifest_dir, DevTheme::Dark);

        assert_eq!(
            path,
            Path::new("/repo/apps/ai-profiles/src-tauri/../../landing/public/screenshot-dark.png")
        );
    }

    #[test]
    fn screencapture_args_capture_the_window_silently_as_a_png() {
        let args = screencapture_args(4242, Path::new("/tmp/shot.png"));

        assert_eq!(
            args,
            ["-x", "-l", "4242", "-t", "png", "/tmp/shot.png"].map(OsString::from)
        );
    }
}

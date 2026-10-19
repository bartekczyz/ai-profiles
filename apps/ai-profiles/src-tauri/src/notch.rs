// SPDX-License-Identifier: MIT

//! The notch HUD: every profile's weekly limit, at a glance, from the notch
//! of a MacBook's built-in display.
//!
//! Collapsed, the panel is the notch row: the notch plus a narrow wing either
//! side, pure black, so the panel and the camera cutout read as one shape.
//! The pointer resting in the notch or on a wing expands the panel downward
//! with one weekly meter per profile. It collapses again a moment after the
//! pointer leaves.
//!
//! Adapted from the island of antiburn (MIT, Copyright (c) 2026 Cadence AI
//! (Vic) Pty Ltd, <https://github.com/antiburn/antiburn>), without its
//! floating, docking and drag placements: here the HUD only ever lives in the
//! notch, and a Mac without one shows nothing.
//!
//! The notch is read from `NSScreen` on the main thread and cached; the
//! watcher on the async runtime only reads the cache.

use serde::Serialize;

/// The label of the HUD window. The frontend serves it `notch.html`.
pub const LABEL: &str = "notch";

/// The narrowest drawable strip either side of the notch, in logical points:
/// room for one small figure and its padding beside the camera.
const WING: f64 = 30.0;

/// The widest the folded wings grow to fit the profile summary, in logical
/// points.
const MAX_WING: f64 = 180.0;

/// A transparent gutter either side of the panel, in logical points, where
/// the top corners curve outward into the bezel.
const FILLET: f64 = 19.0;

/// The narrowest the expanded body gets, in logical points: room for a
/// profile name and its percentage on one line.
const EXPANDED_MIN_WIDTH: f64 = 320.0;

/// How far the notch spreads over the cutout, in logical points. The cutout's
/// edge is anti-aliased; two extra points a side keep it black.
const NOTCH_SPREAD: f64 = 4.0;

/// How far the hover hotspot reaches past the notch's sides and below it, in
/// logical points. The pointer hides inside the notch, so a pointer that
/// skims it still counts.
const HOTSPOT_SIDE: f64 = 10.0;
const HOTSPOT_BELOW: f64 = 5.0;

/// The event that carries [`NotchState`] to the HUD webview.
#[cfg(target_os = "macos")]
const STATE_EVENT: &str = "notch:state";

/// The event that tells the HUD webview which limit crossed.
#[cfg(target_os = "macos")]
const ALERT_EVENT: &str = "notch:alert";

/// The event that asks the main window to show a profile.
#[cfg(target_os = "macos")]
const OPEN_PROFILE_EVENT: &str = "notch:open-profile";

/// What the panel is doing, as the webview renders it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NotchPhase {
    /// No display has a notch, or the HUD is off.
    Hidden,
    /// The notch row alone.
    Collapsed,
    /// The meters below the notch row.
    Expanded,
}

/// The geometry the webview renders, in logical points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotchState {
    pub phase: NotchPhase,
    /// The notch width, which the webview leaves empty.
    pub notch: f64,
    /// The notch height: the collapsed row.
    pub height: f64,
    pub wing: f64,
    pub fillet: f64,
    /// The width of the black panel, without the fillet gutters.
    pub body_width: f64,
    /// The most the expanded panel can grow below the notch row.
    pub body_max_height: f64,
}

impl NotchState {
    pub const fn hidden() -> Self {
        Self {
            phase: NotchPhase::Hidden,
            notch: 0.0,
            height: 0.0,
            wing: 0.0,
            fillet: 0.0,
            body_width: 0.0,
            body_max_height: 0.0,
        }
    }
}

/// A rectangle in AppKit screen points: origin at the bottom-left of the
/// primary display, `y` growing upward, as `NSScreen` and `NSEvent` report
/// it. One space for every display, whatever its backing scale, so nothing
/// converts between pixels and points.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl Rect {
    fn right(&self) -> f64 {
        self.x + self.width
    }

    fn top(&self) -> f64 {
        self.y + self.height
    }

    /// True when `point` is inside, its top edge included: the pointer
    /// pressed against the top of the screen sits on it.
    fn contains(&self, (x, y): (f64, f64)) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y <= self.top()
    }
}

/// A notch and the area of its display below the menu bar, in screen points.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Notch {
    rect: Rect,
    available: Rect,
}

impl Notch {
    /// The black panel's width in `phase`, clamped to the display.
    fn body_width(&self, phase: NotchPhase, wing: f64) -> f64 {
        let collapsed = self.rect.width + 2.0 * wing;
        let wanted = if phase == NotchPhase::Expanded {
            collapsed.max(EXPANDED_MIN_WIDTH)
        } else {
            collapsed
        };
        wanted.min(self.available.width - 2.0 * FILLET)
    }

    /// The room below the notch row, down to the bottom of the display.
    fn body_max_height(&self) -> f64 {
        (self.rect.y - self.available.y).max(0.0)
    }

    fn state(&self, phase: NotchPhase, wing: f64) -> NotchState {
        NotchState {
            phase,
            notch: self.rect.width,
            height: self.rect.height,
            wing,
            fillet: FILLET,
            body_width: self.body_width(phase, wing),
            body_max_height: self.body_max_height(),
        }
    }

    /// The window frame in `phase`: hung from the top of the display,
    /// centred on the notch. `content` is the height the webview measured
    /// below the notch row.
    fn frame(&self, phase: NotchPhase, content: f64, wing: f64) -> Rect {
        let width = self.body_width(phase, wing) + 2.0 * FILLET;
        let height = if phase == NotchPhase::Expanded {
            self.rect.height + content.clamp(0.0, self.body_max_height())
        } else {
            self.rect.height
        };
        let centre = self.rect.x + self.rect.width / 2.0;
        let rightmost = (self.available.right() - width).max(self.available.x);
        Rect {
            x: (centre - width / 2.0).clamp(self.available.x, rightmost),
            y: self.rect.top() - height,
            width,
            height,
        }
    }

    /// True when `cursor` is in the notch or just around it.
    fn in_hotspot(&self, cursor: (f64, f64)) -> bool {
        Rect {
            x: self.rect.x - HOTSPOT_SIDE,
            y: self.rect.y - HOTSPOT_BELOW,
            width: self.rect.width + 2.0 * HOTSPOT_SIDE,
            height: self.rect.height + HOTSPOT_BELOW,
        }
        .contains(cursor)
    }
}

/// True when `actual` is more than half a point from `expected` on any edge.
fn frame_drifted(expected: &Rect, actual: &Rect) -> bool {
    [
        expected.x - actual.x,
        expected.y - actual.y,
        expected.width - actual.width,
        expected.height - actual.height,
    ]
    .iter()
    .any(|delta| delta.abs() > 0.5)
}

/// The notch of a display, or `None` when the display has none.
///
/// `display` is the display frame. `safe_top` is the top safe-area inset and
/// `aux_left` and `aux_right` are the widths of the menu bar areas either
/// side of the camera housing. The notch is the gap between the two areas,
/// spread over the cutout's edge.
fn notch_rect(display: &Rect, safe_top: f64, aux_left: f64, aux_right: f64) -> Option<Rect> {
    if safe_top <= 0.0 {
        return None;
    }
    let gap = display.width - aux_left - aux_right;
    if gap <= 0.0 {
        return None;
    }
    Some(Rect {
        x: display.x + aux_left - NOTCH_SPREAD / 2.0,
        y: display.top() - safe_top,
        width: gap + NOTCH_SPREAD,
        height: safe_top,
    })
}

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, MutexGuard};
    use std::time::{Duration, Instant};

    use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

    use super::{
        frame_drifted, notch_rect, Notch, NotchPhase, NotchState, Rect, ALERT_EVENT, LABEL,
        MAX_WING, OPEN_PROFILE_EVENT, STATE_EVENT, WING,
    };
    use crate::usage_alerts::UsageAlert;

    /// How often the watcher reads the cursor.
    const POLL: Duration = Duration::from_millis(100);
    /// How long the pointer rests on the notch before the panel opens.
    const HOVER_HOLD: Duration = Duration::from_millis(150);
    /// How long the panel stays open after the pointer leaves it.
    const LINGER: Duration = Duration::from_millis(1200);
    /// How long a usage alert holds the panel open, pointer or not.
    const ALERT_HOLD: Duration = Duration::from_secs(8);
    /// How often the watcher reads the displays again and checks the panel
    /// is still on the notch, in polls: a lid that closes or a resolution
    /// change moves or removes the notch, and macOS moves windows itself.
    const DISPLAY_EVERY: u32 = 10;

    #[derive(Debug)]
    struct Hud {
        notch: Option<Notch>,
        expanded: bool,
        /// The height the webview measured below the notch row.
        content: f64,
        /// A usage alert keeps the panel open until then.
        held_until: Option<Instant>,
        /// The folded wings' width, which the profile summary sets.
        wing: f64,
    }

    static HUD: Mutex<Hud> = Mutex::new(Hud {
        notch: None,
        expanded: false,
        content: 0.0,
        held_until: None,
        wing: WING,
    });

    /// Bumped by every open and close, so a watcher of an older window stops.
    static GENERATION: AtomicU64 = AtomicU64::new(0);

    fn hud() -> MutexGuard<'static, Hud> {
        HUD.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn phase(hud: &Hud) -> NotchPhase {
        match (hud.notch, hud.expanded) {
            (None, _) => NotchPhase::Hidden,
            (Some(_), false) => NotchPhase::Collapsed,
            (Some(_), true) => NotchPhase::Expanded,
        }
    }

    /// The state the webview should render now.
    pub fn state() -> NotchState {
        let hud = hud();
        match hud.notch {
            Some(notch) => notch.state(phase(&hud), hud.wing),
            None => NotchState::hidden(),
        }
    }

    /// Show the HUD, or take it away. Idempotent.
    pub fn set_enabled(app: &AppHandle, enabled: bool) -> tauri::Result<()> {
        if enabled {
            open(app)
        } else {
            close(app);
            Ok(())
        }
    }

    fn open(app: &AppHandle) -> tauri::Result<()> {
        if app.get_webview_window(LABEL).is_some() {
            return Ok(());
        }
        let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("notch.html".into()))
            .title("ai-profiles notch")
            .inner_size(1.0, 1.0)
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .focused(false)
            .visible(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .accept_first_mouse(true)
            .build()?;
        let generation = GENERATION.fetch_add(1, Ordering::AcqRel) + 1;
        {
            let mut hud = hud();
            hud.expanded = false;
        }
        let native = window.clone();
        window.run_on_main_thread(move || {
            raise_above_menu_bar(&native);
            refresh_notch();
            apply(&native);
        })?;
        spawn_watcher(app.clone(), window, generation);
        Ok(())
    }

    /// Close the HUD window, if it is open.
    pub fn close(app: &AppHandle) {
        GENERATION.fetch_add(1, Ordering::AcqRel);
        if let Some(window) = app.get_webview_window(LABEL) {
            let _ = window.destroy();
        }
    }

    /// Record the height the webview measured below the notch row, and fit
    /// an open panel to it.
    pub fn set_content_height(app: &AppHandle, height: f64) {
        if !height.is_finite() {
            return;
        }
        let expanded = {
            let mut hud = hud();
            hud.content = height.max(0.0);
            hud.expanded
        };
        if expanded {
            if let Some(window) = app.get_webview_window(LABEL) {
                apply(&window);
            }
        }
    }

    /// Fit the folded wings to the profile summary the webview measured.
    pub fn set_wing_width(app: &AppHandle, width: f64) {
        if !width.is_finite() {
            return;
        }
        {
            let mut hud = hud();
            let wing = width.clamp(WING, MAX_WING);
            if (hud.wing - wing).abs() < 0.5 {
                return;
            }
            hud.wing = wing;
        }
        if let Some(window) = app.get_webview_window(LABEL) {
            apply(&window);
        }
    }

    /// True when the panel is open in a notch, so an alert has somewhere to
    /// show.
    pub fn available() -> bool {
        hud().notch.is_some()
    }

    /// Drop the panel open for a usage alert, and tell the webview which
    /// limit crossed.
    pub fn alert(app: &AppHandle, alert: UsageAlert) {
        let Some(window) = app.get_webview_window(LABEL) else {
            return;
        };
        {
            let mut hud = hud();
            if hud.notch.is_none() {
                return;
            }
            hud.held_until = Some(Instant::now() + ALERT_HOLD);
        }
        let _ = window.emit_to(LABEL, ALERT_EVENT, alert);
        set_expanded(&window, true);
    }

    /// Bring the main window forward on profile `id`, and fold the panel.
    pub fn open_profile(app: &AppHandle, id: String) {
        if let Some(main) = app.get_webview_window("main") {
            let _ = main.unminimize();
            let _ = main.show();
            let _ = main.set_focus();
        }
        let _ = app.emit_to("main", OPEN_PROFILE_EVENT, id);
        if let Some(window) = app.get_webview_window(LABEL) {
            set_expanded(&window, false);
        }
    }

    /// Put the window where the cached notch and phase say, and tell the
    /// webview.
    fn apply(window: &WebviewWindow) {
        let (notch, phase, content, wing) = {
            let hud = hud();
            (hud.notch, phase(&hud), hud.content, hud.wing)
        };
        let Some(notch) = notch else {
            let _ = window.hide();
            let _ = window.emit_to(LABEL, STATE_EVENT, NotchState::hidden());
            return;
        };
        let frame = notch.frame(phase, content, wing);
        // A collapsed panel lets clicks through to the menu bar; an open one
        // takes them, so a profile row can be clicked.
        let _ = window.set_ignore_cursor_events(phase != NotchPhase::Expanded);
        let _ = window.emit_to(LABEL, STATE_EVENT, notch.state(phase, wing));
        place(window, frame);
    }

    fn set_expanded(window: &WebviewWindow, expanded: bool) {
        {
            let mut hud = hud();
            if !expanded {
                hud.held_until = None;
            }
            if hud.expanded == expanded || hud.notch.is_none() {
                return;
            }
            hud.expanded = expanded;
        }
        apply(window);
    }

    /// The window frame now, for the pointer test.
    fn current_frame() -> Option<Rect> {
        let hud = hud();
        hud.notch
            .map(|notch| notch.frame(phase(&hud), hud.content, hud.wing))
    }

    /// Open the panel on a rest over the notch; fold it a moment after the
    /// pointer leaves. Every so often, read the displays again.
    fn spawn_watcher(app: AppHandle, window: WebviewWindow, generation: u64) {
        tauri::async_runtime::spawn(async move {
            let mut rested_since: Option<Instant> = None;
            let mut left_at: Option<Instant> = None;
            let mut polls: u32 = 0;
            loop {
                tokio::time::sleep(POLL).await;
                if GENERATION.load(Ordering::Acquire) != generation
                    || app.get_webview_window(LABEL).is_none()
                {
                    return;
                }
                polls = polls.wrapping_add(1);
                if polls.is_multiple_of(DISPLAY_EVERY) {
                    let native = window.clone();
                    let _ = window.run_on_main_thread(move || {
                        // macOS moves windows itself when displays change —
                        // sleep, the lid, a monitor plugged in or rearranged —
                        // and pushes one over the menu bar down into the
                        // screen. Put the panel back whenever it is not where
                        // it belongs, not only when the notch changed.
                        if refresh_notch() || drifted(&native) {
                            apply(&native);
                        }
                    });
                }
                let cursor = cursor_location();
                let (notch, expanded, held) = {
                    let hud = hud();
                    let held = hud.held_until.is_some_and(|until| Instant::now() < until);
                    (hud.notch, hud.expanded, held)
                };
                let Some(notch) = notch else {
                    rested_since = None;
                    left_at = None;
                    continue;
                };
                let over_panel = current_frame().is_some_and(|frame| frame.contains(cursor));
                let near = notch.in_hotspot(cursor) || over_panel;
                if expanded {
                    rested_since = None;
                    if near || held {
                        left_at = None;
                    } else if left_at.get_or_insert_with(Instant::now).elapsed() >= LINGER {
                        left_at = None;
                        set_expanded(&window, false);
                    }
                } else {
                    left_at = None;
                    if !near {
                        rested_since = None;
                    } else if rested_since.get_or_insert_with(Instant::now).elapsed() >= HOVER_HOLD
                    {
                        rested_since = None;
                        set_expanded(&window, true);
                    }
                }
            }
        });
    }

    /// Put the panel at `frame` and bring it on screen, without making
    /// ai-profiles the active app.
    ///
    /// The frame goes straight to AppKit in screen points. Tauri's position
    /// setters convert pixels with the scale of whichever display the window
    /// is on at the time, which put the panel in the middle of a screen when
    /// the displays differed. `WebviewWindow::show` makes the window key,
    /// which activates the app and raises the main window with it.
    fn place(window: &WebviewWindow, frame: Rect) {
        let native = window.clone();
        let _ = window.run_on_main_thread(move || {
            use objc2_app_kit::NSWindow;
            use objc2_foundation::{NSPoint, NSRect, NSSize};

            let Ok(pointer) = native.ns_window() else {
                return;
            };
            // SAFETY: Tauri hands out the live NSWindow of this webview
            // window, and this runs on the main thread.
            let ns_window: &NSWindow = unsafe { &*pointer.cast::<NSWindow>() };
            ns_window.setFrame_display(
                NSRect::new(
                    NSPoint::new(frame.x, frame.y),
                    NSSize::new(frame.width, frame.height),
                ),
                true,
            );
            ns_window.orderFrontRegardless();
        });
    }

    /// True when the window is not where the cached notch puts it. Main
    /// thread only.
    fn drifted(window: &WebviewWindow) -> bool {
        use objc2_app_kit::NSWindow;

        let Some(expected) = current_frame() else {
            return false;
        };
        let Ok(pointer) = window.ns_window() else {
            return false;
        };
        // SAFETY: Tauri hands out the live NSWindow of this webview window,
        // and this runs on the main thread.
        let ns_window: &NSWindow = unsafe { &*pointer.cast::<NSWindow>() };
        let actual = rect(ns_window.frame());
        frame_drifted(&expected, &actual)
    }

    /// The pointer in screen points.
    fn cursor_location() -> (f64, f64) {
        let point = objc2_app_kit::NSEvent::mouseLocation();
        (point.x, point.y)
    }

    /// Lift the window over the menu bar, onto every Space, fullscreen ones
    /// included, and out of the window cycle. Main thread only.
    fn raise_above_menu_bar(window: &WebviewWindow) {
        use objc2_app_kit::{NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior};

        let Ok(pointer) = window.ns_window() else {
            return;
        };
        // SAFETY: Tauri hands out the live NSWindow of this webview window,
        // and this runs on the main thread, where AppKit wants it touched.
        let ns_window: &NSWindow = unsafe { &*pointer.cast::<NSWindow>() };
        ns_window.setLevel(NSScreenSaverWindowLevel);
        ns_window.setHasShadow(false);
        ns_window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
    }

    /// Read the notch again and cache it. Returns true when it changed. Main
    /// thread only: off it the cache keeps its value.
    fn refresh_notch() -> bool {
        let Some(read) = objc2_foundation::MainThreadMarker::new().map(read_notch) else {
            return false;
        };
        let mut hud = hud();
        if hud.notch == read {
            return false;
        }
        hud.notch = read;
        if read.is_none() {
            hud.expanded = false;
        }
        true
    }

    /// The notch of the first display that has one.
    fn read_notch(mtm: objc2_foundation::MainThreadMarker) -> Option<Notch> {
        use objc2_app_kit::NSScreen;

        NSScreen::screens(mtm).iter().find_map(|screen| {
            let insets = screen.safeAreaInsets();
            let left = screen.auxiliaryTopLeftArea().size.width;
            let right = screen.auxiliaryTopRightArea().size.width;
            notch_rect(&rect(screen.frame()), insets.top, left, right).map(|notch| Notch {
                rect: notch,
                available: rect(screen.visibleFrame()),
            })
        })
    }

    fn rect(frame: objc2_foundation::NSRect) -> Rect {
        Rect {
            x: frame.origin.x,
            y: frame.origin.y,
            width: frame.size.width,
            height: frame.size.height,
        }
    }
}

/// The HUD needs a notch, which only a Mac has.
#[cfg(not(target_os = "macos"))]
pub fn set_enabled(_app: &tauri::AppHandle, _enabled: bool) -> tauri::Result<()> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn close(_app: &tauri::AppHandle) {}

#[cfg(not(target_os = "macos"))]
pub fn state() -> NotchState {
    NotchState::hidden()
}

#[cfg(not(target_os = "macos"))]
pub fn set_content_height(_app: &tauri::AppHandle, _height: f64) {}

#[cfg(not(target_os = "macos"))]
pub fn set_wing_width(_app: &tauri::AppHandle, _width: f64) {}

#[cfg(not(target_os = "macos"))]
pub fn available() -> bool {
    false
}

#[cfg(not(target_os = "macos"))]
pub fn open_profile(_app: &tauri::AppHandle, _id: String) {}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 14-inch MacBook Pro display, 1512 x 982 points, as the primary
    /// display: its bottom-left is the origin.
    const DISPLAY: Rect = Rect {
        x: 0.0,
        y: 0.0,
        width: 1512.0,
        height: 982.0,
    };

    /// The display less the 32-point menu bar.
    const VISIBLE: Rect = Rect {
        height: 950.0,
        ..DISPLAY
    };

    fn notch_on(display: Rect, visible: Rect) -> Notch {
        Notch {
            rect: notch_rect(&display, 32.0, 656.0, 656.0).expect("a notched display"),
            available: visible,
        }
    }

    fn notch() -> Notch {
        notch_on(DISPLAY, VISIBLE)
    }

    fn centre(rect: &Rect) -> f64 {
        rect.x + rect.width / 2.0
    }

    #[test]
    fn the_notch_is_the_gap_between_the_menu_bar_areas_plus_the_spread() {
        let rect = notch().rect;
        assert_eq!(rect.x, 656.0 - 2.0);
        assert_eq!(rect.width, 1512.0 - 656.0 - 656.0 + 4.0);
        // It hangs from the top of the display.
        assert_eq!(rect.top(), 982.0);
        assert_eq!(rect.height, 32.0);
    }

    #[test]
    fn a_display_without_a_safe_area_has_no_notch() {
        assert_eq!(notch_rect(&DISPLAY, 0.0, 0.0, 0.0), None);
        // Menu bar areas that meet leave no gap.
        assert_eq!(notch_rect(&DISPLAY, 32.0, 756.0, 756.0), None);
    }

    #[test]
    fn collapsed_the_window_is_the_notch_row_with_a_wing_and_gutter_each_side() {
        let notch = notch();
        let frame = notch.frame(NotchPhase::Collapsed, 400.0, WING);
        assert_eq!(frame.width, 204.0 + 2.0 * (WING + FILLET));
        assert_eq!(frame.height, 32.0);
        assert_eq!(frame.top(), 982.0);
        assert_eq!(centre(&frame), centre(&notch.rect));
    }

    #[test]
    fn expanded_the_window_grows_down_to_the_content_and_stays_centred() {
        let notch = notch();
        let frame = notch.frame(NotchPhase::Expanded, 180.0, WING);
        assert_eq!(frame.width, EXPANDED_MIN_WIDTH + 2.0 * FILLET);
        assert_eq!(frame.height, 32.0 + 180.0);
        assert_eq!(frame.top(), 982.0);
        assert_eq!(centre(&frame), centre(&notch.rect));
    }

    #[test]
    fn expanded_content_is_capped_at_the_bottom_of_the_display() {
        let notch = notch();
        assert_eq!(notch.body_max_height(), 950.0);
        let frame = notch.frame(NotchPhase::Expanded, 5000.0, WING);
        assert_eq!(frame.height, 32.0 + 950.0);
        assert_eq!(frame.y, 0.0);
    }

    #[test]
    fn a_notched_display_beside_a_taller_primary_keeps_the_panel_on_its_own_top() {
        // The built-in display sits right of a 1920 x 1080 primary, its
        // bottom 100 points above the primary's: the arrangement in which
        // the panel used to land mid-screen.
        let display = Rect {
            x: 1920.0,
            y: 100.0,
            ..DISPLAY
        };
        let visible = Rect {
            height: 950.0,
            ..display
        };
        let notch = notch_on(display, visible);
        for phase in [NotchPhase::Collapsed, NotchPhase::Expanded] {
            let frame = notch.frame(phase, 200.0, 60.0);
            assert_eq!(frame.top(), display.top());
            assert_eq!(centre(&frame), 1920.0 + 1512.0 / 2.0);
        }
    }

    #[test]
    fn the_hotspot_reaches_a_little_past_the_notch() {
        let notch = notch();
        let rect = notch.rect;
        assert!(notch.in_hotspot((rect.x + 5.0, 975.0)));
        // Pressed against the top of the screen.
        assert!(notch.in_hotspot((rect.x + 5.0, 982.0)));
        assert!(notch.in_hotspot((rect.x - 9.0, 975.0)));
        assert!(!notch.in_hotspot((rect.x - 11.0, 975.0)));
        assert!(notch.in_hotspot((rect.x + 5.0, rect.y - 4.0)));
        assert!(!notch.in_hotspot((rect.x + 5.0, rect.y - 6.0)));
    }

    #[test]
    fn the_pointer_is_over_the_panel_only_inside_its_frame() {
        let frame = notch().frame(NotchPhase::Expanded, 100.0, WING);
        assert!(frame.contains((frame.x + 1.0, frame.top() - 1.0)));
        assert!(frame.contains((frame.x + 1.0, frame.y)));
        assert!(!frame.contains((frame.x + 1.0, frame.y - 1.0)));
        assert!(!frame.contains((frame.x - 1.0, frame.top() - 1.0)));
    }

    #[test]
    fn wider_wings_widen_the_folded_panel_and_the_open_one_keeps_its_minimum() {
        let notch = notch();
        let folded = notch.frame(NotchPhase::Collapsed, 0.0, 90.0);
        assert_eq!(folded.width, 204.0 + 2.0 * (90.0 + FILLET));
        assert_eq!(centre(&folded), centre(&notch.rect));
        let open = notch.frame(NotchPhase::Expanded, 100.0, 90.0);
        assert_eq!(open.width, 204.0 + 2.0 * (90.0 + FILLET));
        let narrow = notch.frame(NotchPhase::Expanded, 100.0, WING);
        assert_eq!(narrow.width, EXPANDED_MIN_WIDTH + 2.0 * FILLET);
    }

    #[test]
    fn the_webview_state_is_in_points() {
        let state = notch().state(NotchPhase::Collapsed, WING);
        assert_eq!(state.notch, 204.0);
        assert_eq!(state.height, 32.0);
        assert_eq!(state.body_width, 204.0 + 2.0 * WING);
        let json = serde_json::to_value(state).expect("serializable");
        assert_eq!(json["phase"], "collapsed");
        assert_eq!(json["bodyWidth"], 204.0 + 2.0 * WING);
    }

    #[test]
    fn a_frame_moved_by_macos_counts_as_drifted() {
        let expected = notch().frame(NotchPhase::Collapsed, 0.0, WING);
        assert!(!frame_drifted(&expected, &expected));
        // Rounding in AppKit is not drift.
        let rounded = Rect {
            x: expected.x + 0.25,
            ..expected
        };
        assert!(!frame_drifted(&expected, &rounded));
        // Pushed down into the screen, as a display change does.
        let pushed = Rect {
            y: expected.y - 300.0,
            ..expected
        };
        assert!(frame_drifted(&expected, &pushed));
    }
}

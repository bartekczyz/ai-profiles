// SPDX-License-Identifier: MIT

//! Usage alerts: the notch panel drops open on its own when a profile's
//! weekly or 5-hour limit crosses a threshold (50, 75, 90 and 100% unless the
//! user picks others).
//!
//! The rules follow antiburn's usage milestones (MIT,
//! <https://github.com/antiburn/antiburn>):
//!
//! - A window's thresholds fire once per reset. The reset time is part of the
//!   window's key, so a new week or session arms them all again.
//! - The first reading of a window records what it has already crossed
//!   without alerting, so a launch at 80% replays nothing. The same goes for
//!   a threshold switched on after usage passed it.
//! - Several crossings at once alert one at a time: the window furthest ahead
//!   of an even pace first, the rest on later readings.
//! - An alert says whether usage is ahead of a straight-line pace through the
//!   window.
//!
//! Readings come from every usage fetch a window makes, and from a background
//! check every few minutes. The background check runs only while someone is
//! at the Mac — input in the last ten minutes — and never refreshes a Claude
//! token or marks a sign-in dead, so an unattended night cannot sign a
//! profile out.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::app_kind::AppKind;
use crate::usage::{ProfileUsage, QuotaUsage, Window};

const WEEK_MINUTES: i64 = 7 * 24 * 60;
/// Claude's session window. Codex states its own durations.
const CLAUDE_SESSION_MINUTES: i64 = 5 * 60;

/// The thresholds a user starts with, in percent used.
pub const DEFAULT_THRESHOLDS: [u8; 4] = [50, 75, 90, 100];

/// Which of a profile's limits crossed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertWindow {
    Weekly,
    Session,
}

/// One limit of one profile, as the alerts see it.
#[derive(Debug, Clone, PartialEq)]
struct Reading {
    window: AlertWindow,
    /// The window's length, for the label the panel shows: `5h`, `W`.
    minutes: i64,
    used: f64,
    /// The share of the window gone, 0-100.
    elapsed: f64,
    resets_at: i64,
}

/// What the notch panel shows when a limit crosses.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageAlert {
    pub profile_id: String,
    pub window: AlertWindow,
    /// The window's length in minutes: 10080 for a week.
    pub window_minutes: i64,
    pub threshold: u8,
    pub used: f64,
    pub elapsed: f64,
    /// Usage is ahead of a straight-line pace through the window.
    pub ahead: bool,
}

/// The share of a window gone by `now`, 0-100.
fn elapsed_percent(resets_at: DateTime<Utc>, minutes: i64, now: DateTime<Utc>) -> f64 {
    let remaining = (resets_at - now).num_seconds() as f64;
    ((1.0 - remaining / (minutes * 60) as f64) * 100.0).clamp(0.0, 100.0)
}

/// The readings of one usage snapshot: the weekly limit and the session.
fn readings(app: AppKind, quota: &QuotaUsage, now: DateTime<Utc>) -> Vec<Reading> {
    let windows: Vec<(AlertWindow, i64, &Window)> = match app {
        AppKind::Claude => [
            quota
                .secondary
                .as_ref()
                .map(|window| (AlertWindow::Weekly, WEEK_MINUTES, window)),
            quota
                .primary
                .as_ref()
                .map(|window| (AlertWindow::Session, CLAUDE_SESSION_MINUTES, window)),
        ]
        .into_iter()
        .flatten()
        .collect(),
        AppKind::Codex => [&quota.primary, &quota.secondary]
            .into_iter()
            .flatten()
            .filter_map(|window| {
                let minutes = window.window_duration_mins?;
                let kind = if minutes == WEEK_MINUTES {
                    AlertWindow::Weekly
                } else {
                    AlertWindow::Session
                };
                Some((kind, minutes, window))
            })
            .collect(),
    };
    windows
        .into_iter()
        .filter_map(|(window, minutes, raw)| {
            let used = f64::from(raw.utilization?);
            let resets_at = DateTime::parse_from_rfc3339(raw.resets_at.as_deref()?)
                .ok()?
                .with_timezone(&Utc);
            let elapsed = elapsed_percent(resets_at, minutes, now);
            used.is_finite().then_some(Reading {
                window,
                minutes,
                used,
                elapsed,
                resets_at: resets_at.timestamp(),
            })
        })
        .collect()
}

/// What each window has already been alerted or primed up to.
#[derive(Debug, Default)]
struct Ledger {
    /// The highest usage recorded per `profile:window:reset`.
    seen: HashMap<String, f64>,
}

impl Ledger {
    /// Record `profile_id`'s readings and return the crossings they make,
    /// best first, without recording those: the caller records the one it
    /// shows, so the rest alert on a later reading.
    fn crossings(
        &mut self,
        profile_id: &str,
        readings: &[Reading],
        thresholds: &[u8],
    ) -> Vec<(String, UsageAlert)> {
        let mut found = Vec::new();
        for reading in readings {
            let key = format!("{profile_id}:{:?}:{}", reading.window, reading.resets_at);
            let Some(&before) = self.seen.get(&key) else {
                // First sight: what is already crossed stays quiet.
                self.seen.insert(key, reading.used);
                continue;
            };
            let crossed = thresholds
                .iter()
                .copied()
                .filter(|&threshold| {
                    before < f64::from(threshold) && reading.used >= f64::from(threshold)
                })
                .max();
            match crossed {
                Some(threshold) => found.push((
                    key,
                    UsageAlert {
                        profile_id: profile_id.to_string(),
                        window: reading.window,
                        window_minutes: reading.minutes,
                        threshold,
                        used: reading.used,
                        elapsed: reading.elapsed,
                        ahead: reading.used.round() > reading.elapsed,
                    },
                )),
                None => {
                    self.seen.insert(key, before.max(reading.used));
                }
            }
        }
        found
    }

    /// Record the alert that was shown, and every threshold below it.
    fn shown(&mut self, key: String, used: f64) {
        let entry = self.seen.entry(key).or_insert(used);
        *entry = entry.max(used);
    }
}

/// The crossing to show first: the one furthest ahead of pace, then the
/// highest threshold.
fn best(crossings: Vec<(String, UsageAlert)>) -> Option<(String, UsageAlert)> {
    crossings.into_iter().max_by(|(_, a), (_, b)| {
        (a.used - a.elapsed)
            .total_cmp(&(b.used - b.elapsed))
            .then(a.threshold.cmp(&b.threshold))
    })
}

/// The thresholds a stored setting names: 1-100, sorted, without repeats.
pub fn normalise_thresholds(thresholds: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = thresholds
        .iter()
        .copied()
        .filter(|threshold| (1..=100).contains(threshold))
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(target_os = "macos")]
pub use macos::{observe, spawn};

#[cfg(target_os = "macos")]
mod macos {
    use std::sync::{LazyLock, Mutex, MutexGuard};
    use std::time::Duration;

    use chrono::Utc;
    use tauri::AppHandle;

    use super::{best, readings, Ledger, ProfileUsage};
    use crate::app_kind::{default_id, spec, AppKind};
    use crate::commands::{fetch_profile_usage, resolve_app, Attendance};
    use crate::{app_state, notch, paths, profiles};

    /// The background check's period.
    const TICK: Duration = Duration::from_secs(5 * 60);
    /// The first check waits for the app to settle.
    const STARTUP_DELAY: Duration = Duration::from_secs(60);
    /// How recent the last keyboard or pointer input must be for someone to
    /// count as at the Mac.
    const PRESENT_WITHIN_SECS: f64 = 10.0 * 60.0;

    static LEDGER: LazyLock<Mutex<Ledger>> = LazyLock::new(Mutex::default);

    fn ledger() -> MutexGuard<'static, Ledger> {
        LEDGER
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Feed one reading to the alerts, and show a crossing it makes.
    pub fn observe(app: &AppHandle, profile_id: &str, usage: &ProfileUsage) {
        let Ok(kind) = resolve_app(profile_id) else {
            return;
        };
        let Some(quota) = usage.quota.as_ref() else {
            return;
        };
        let state = app_state::load().unwrap_or_default();
        let readings = readings(kind, quota, Utc::now());
        let mut ledger = ledger();
        let crossings = ledger.crossings(profile_id, &readings, &state.alert_thresholds());
        let Some((key, alert)) = best(crossings) else {
            return;
        };
        // A crossing nobody can be shown is spent all the same: it must not
        // replay when the notch or the profile comes back.
        ledger.shown(key, alert.used);
        drop(ledger);
        if state.notch_hud_off {
            return;
        }
        notch::alert(app, alert);
    }

    /// Start the background check.
    pub fn spawn(app: AppHandle) {
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(STARTUP_DELAY).await;
            loop {
                check(&app).await;
                tokio::time::sleep(TICK).await;
            }
        });
    }

    async fn check(app: &AppHandle) {
        let state = app_state::load().unwrap_or_default();
        // The alert shows in the notch, so there is nothing to check for
        // without one, or with no thresholds, or with nobody at the Mac.
        if state.notch_hud_off
            || state.alert_thresholds().is_empty()
            || !notch::available()
            || seconds_since_input() > PRESENT_WITHIN_SECS
        {
            return;
        }
        for profile_id in watched_profiles() {
            if let Ok(usage) = fetch_profile_usage(&profile_id, Attendance::Background).await {
                observe(app, &profile_id, &usage);
            }
        }
    }

    /// The profiles the notch shows: every one with a CLI and usage, and
    /// each stock install that has signed in.
    fn watched_profiles() -> Vec<String> {
        let mut ids: Vec<String> = [AppKind::Claude, AppKind::Codex]
            .into_iter()
            .filter(|kind| spec(*kind).has_usage)
            .filter(|kind| paths::stock_cli_config_dir(spec(*kind)).is_ok_and(|dir| dir.exists()))
            .map(default_id)
            .collect();
        ids.extend(
            profiles::load()
                .unwrap_or_default()
                .into_iter()
                .filter(|profile| profile.surfaces.cli && spec(profile.app).has_usage)
                .map(|profile| profile.id),
        );
        ids
    }

    /// Seconds since the last keyboard, pointer or trackpad input.
    fn seconds_since_input() -> f64 {
        #[link(name = "CoreGraphics", kind = "framework")]
        extern "C" {
            fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
        }
        const COMBINED_SESSION_STATE: i32 = 0;
        const ANY_INPUT_EVENT: u32 = u32::MAX;
        // SAFETY: a pure query of the window server with constant arguments.
        unsafe { CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, ANY_INPUT_EVENT) }
    }
}

/// The alerts show in the notch, which only a Mac has.
#[cfg(not(target_os = "macos"))]
pub fn observe(_app: &tauri::AppHandle, _profile_id: &str, _usage: &ProfileUsage) {}

#[cfg(not(target_os = "macos"))]
pub fn spawn(_app: tauri::AppHandle) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-13T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn window(utilization: f32, resets_at: &str, minutes: Option<i64>) -> Window {
        Window {
            window_duration_mins: minutes,
            label: None,
            utilization: Some(utilization),
            resets_at: Some(resets_at.into()),
        }
    }

    fn claude(weekly: f32, session: f32) -> QuotaUsage {
        QuotaUsage {
            // Three and a half days of the week gone; half of the session.
            primary: Some(window(session, "2026-10-13T02:30:00Z", None)),
            secondary: Some(window(weekly, "2026-10-16T12:00:00Z", None)),
            scoped_weekly: Vec::new(),
            spend: None,
            rate_limit_reset_credits: None,
        }
    }

    fn pass(ledger: &mut Ledger, quota: &QuotaUsage) -> Option<UsageAlert> {
        let readings = readings(AppKind::Claude, quota, now());
        let (key, alert) = best(ledger.crossings("p1", &readings, &DEFAULT_THRESHOLDS))?;
        ledger.shown(key, alert.used);
        Some(alert)
    }

    #[test]
    fn readings_carry_the_share_of_the_window_gone() {
        let readings = readings(AppKind::Claude, &claude(40.0, 10.0), now());
        assert_eq!(readings.len(), 2);
        assert_eq!(readings[0].window, AlertWindow::Weekly);
        assert!((readings[0].elapsed - 50.0).abs() < 1e-9);
        assert_eq!(readings[1].window, AlertWindow::Session);
        assert!((readings[1].elapsed - 50.0).abs() < 1e-9);
    }

    #[test]
    fn codex_windows_are_told_apart_by_their_length() {
        let quota = QuotaUsage {
            primary: Some(window(60.0, "2026-10-16T12:00:00Z", Some(WEEK_MINUTES))),
            secondary: Some(window(5.0, "2026-10-13T02:30:00Z", Some(300))),
            scoped_weekly: Vec::new(),
            spend: None,
            rate_limit_reset_credits: None,
        };
        let readings = readings(AppKind::Codex, &quota, now());
        assert_eq!(readings[0].window, AlertWindow::Weekly);
        assert_eq!(readings[1].window, AlertWindow::Session);
        assert_eq!(readings[1].minutes, 300);
    }

    #[test]
    fn the_first_reading_is_quiet_even_past_thresholds() {
        let mut ledger = Ledger::default();
        assert_eq!(pass(&mut ledger, &claude(80.0, 10.0)), None);
        assert_eq!(pass(&mut ledger, &claude(80.0, 10.0)), None);
    }

    #[test]
    fn a_crossing_alerts_once_with_its_highest_threshold() {
        let mut ledger = Ledger::default();
        pass(&mut ledger, &claude(40.0, 10.0));
        let alert = pass(&mut ledger, &claude(92.0, 10.0)).expect("an alert");
        assert_eq!(alert.window, AlertWindow::Weekly);
        assert_eq!(alert.threshold, 90);
        assert!(alert.ahead);
        assert_eq!(pass(&mut ledger, &claude(93.0, 10.0)), None);
    }

    #[test]
    fn behind_pace_is_not_ahead() {
        let mut ledger = Ledger::default();
        pass(&mut ledger, &claude(10.0, 10.0));
        let alert = pass(&mut ledger, &claude(10.0, 50.0)).expect("an alert");
        assert_eq!(alert.window, AlertWindow::Session);
        assert_eq!(alert.threshold, 50);
        assert!(!alert.ahead);
    }

    #[test]
    fn two_crossings_alert_one_at_a_time_furthest_ahead_first() {
        let mut ledger = Ledger::default();
        pass(&mut ledger, &claude(10.0, 10.0));
        // Weekly 76% at half the week is further ahead than session 55% at
        // half the session.
        let first = pass(&mut ledger, &claude(76.0, 55.0)).expect("an alert");
        assert_eq!(first.window, AlertWindow::Weekly);
        let second = pass(&mut ledger, &claude(76.0, 55.0)).expect("the other one");
        assert_eq!(second.window, AlertWindow::Session);
        assert_eq!(pass(&mut ledger, &claude(76.0, 55.0)), None);
    }

    #[test]
    fn a_reset_arms_the_thresholds_again() {
        let mut ledger = Ledger::default();
        pass(&mut ledger, &claude(40.0, 10.0));
        pass(&mut ledger, &claude(60.0, 10.0)).expect("an alert");
        let mut next_week = claude(10.0, 10.0);
        next_week.secondary = Some(window(10.0, "2026-10-23T12:00:00Z", None));
        // A new window is a first sight, then crosses afresh.
        assert_eq!(pass(&mut ledger, &next_week), None);
        next_week.secondary = Some(window(55.0, "2026-10-23T12:00:00Z", None));
        assert_eq!(pass(&mut ledger, &next_week).map(|a| a.threshold), Some(50));
    }

    #[test]
    fn thresholds_are_normalised() {
        assert_eq!(
            normalise_thresholds(&[90, 0, 50, 50, 101, 75]),
            vec![50, 75, 90]
        );
    }
}

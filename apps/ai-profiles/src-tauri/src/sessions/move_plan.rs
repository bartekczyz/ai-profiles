//! What moving a session would do, and what a move did, as the move dialog
//! is shown them: the same for a Claude and a Codex session.

use serde::Serialize;

use crate::sessions::actions::AppToQuit;

/// What a move does with one of its files or folders at the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemAction {
    /// The destination doesn't have it: it is copied.
    Copy,
    /// The destination has the same: it is left alone.
    Same,
    /// The destination has something else there: that is backed up, then
    /// replaced.
    Replace,
}

/// What a move does about the destination's desktop app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DesktopAction {
    /// A record of the session is written, so the app lists it.
    Add,
    /// The app lists the session already.
    AlreadyListed,
    /// The app isn't signed in, so it has no session list to add to.
    SignInNeeded,
    /// The destination has no desktop app.
    NoDesktop,
}

/// One file or folder a move copies, as the plan shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedItem {
    /// Where it goes, relative to the destination's config dir.
    pub path: String,
    /// What the move does with it.
    pub action: ItemAction,
}

/// What moving a session would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MovePlan {
    /// One line saying what moves where: `Moves 3 files from Work to
    /// Personal, and 2 memory files`.
    pub summary: String,
    /// The files and folders the move copies, then the project memory files
    /// it copies, then the transcripts.
    pub items: Vec<PlannedItem>,
    /// The destination has a copy of a transcript that differs and was used
    /// more recently: moving would roll it back, so it takes the user's
    /// say-so.
    pub destination_newer: bool,
    /// What the move does about the destination's desktop app.
    pub desktop: DesktopAction,
    /// Why the move can't be done, when only the user can change that.
    pub blockers: Vec<String>,
    /// The desktop apps that have to quit first, at the source, the
    /// destination or both.
    pub apps_to_quit: Vec<AppToQuit>,
    /// Things worth knowing that don't stop the move.
    pub notes: Vec<String>,
}

/// What a move did that the user should hear about.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveReport {
    /// The memory files both profiles have, differently; the destination's
    /// were kept.
    pub memory_conflicts: Vec<String>,
}

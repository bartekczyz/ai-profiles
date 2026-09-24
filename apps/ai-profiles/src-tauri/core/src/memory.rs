//! Merging a project's memory when a session moves between accounts, the way
//! claudemulti's `sync_memory` does.
//!
//! Claude's auto-memory for a project lives in each account, under
//! `projects/<project>/memory/`, and every session of the project in that
//! account shares it. So a move merges the source's memory into the
//! destination's rather than replacing it. For each file the source has:
//!
//! - missing in the destination: copied;
//! - identical: nothing;
//! - `MEMORY.md`, the index: the newer copy, plus the index lines only the
//!   older copy has, dropping lines whose note no longer exists;
//! - changed on one side only, or on both in different places: a three-way
//!   merge against the version both last had in common (`git merge-file`);
//! - otherwise, a conflict the user decides: keep the newer, take the
//!   source's or the destination's, or a merge Claude writes and the user
//!   reviews.
//!
//! Files only the destination has are never touched, and any destination
//! file that changes is backed up first. The common version is kept outside
//! every account, in `<accounts>/.claudemulti/memory-base/<source project>/`,
//! where claudemulti keeps it too, so Claude never loads it. It is updated
//! from the source for every file on every move.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::session_move::{copy_any, write_into_place};

/// Where the common versions live, relative to the accounts folder.
pub const BASE_DIR: &str = ".claudemulti/memory-base";

/// The index file, at the top of the memory folder.
pub const INDEX: &str = "MEMORY.md";

/// Which side of a move.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    Source,
    Destination,
}

/// What a move does with one memory file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MemoryAction {
    /// The destination doesn't have it: copied.
    Add,
    /// Identical: nothing.
    Same,
    /// The index: merged line by line.
    Index,
    /// Both sides' changes merge cleanly.
    Merge,
    /// Both changed it in the same place, or there's no common version to
    /// merge against: the user decides.
    Conflict,
}

/// One memory file in a move's plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryFile {
    /// Relative to the memory folder.
    pub rel: String,
    pub action: MemoryAction,
    /// Which copy was written later (a tie counts as the source's).
    pub newer: Side,
}

/// What the user decided for a conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "take", content = "text")]
pub enum Decision {
    Source,
    Destination,
    /// Claude's merge, as the user accepted it.
    Merged(String),
}

/// What merging did, one line per file that changed or was looked at, as
/// claudemulti prints it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoryReport {
    pub lines: Vec<String>,
    pub backed_up: bool,
}

/// The files of the memory folder at `memory`, relative to it, sorted, with
/// the top-level index last so it is merged against the final set of notes.
pub fn memory_files(memory: &Path) -> Vec<String> {
    fn walk(dir: &Path, base: &Path, found: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                walk(&path, base, found);
            } else if kind.is_file() {
                if let Ok(rel) = path.strip_prefix(base) {
                    found.push(rel.to_string_lossy().into_owned());
                }
            }
        }
    }
    let mut found = Vec::new();
    walk(memory, memory, &mut found);
    found.sort();
    if let Some(index) = found.iter().position(|rel| rel == INDEX) {
        let index = found.remove(index);
        found.push(index);
    }
    found
}

/// What merging `source` memory into `destination` would do, file by file,
/// with `base` holding the common versions.
pub fn plan_memory(source: &Path, destination: &Path, base: &Path) -> Vec<MemoryFile> {
    memory_files(source)
        .into_iter()
        .map(|rel| {
            let (from, to) = (source.join(&rel), destination.join(&rel));
            let newer = newer_side(&from, &to);
            let action = if fs::symlink_metadata(&to).is_err() {
                MemoryAction::Add
            } else if same_bytes(&from, &to) {
                MemoryAction::Same
            } else if rel == INDEX {
                MemoryAction::Index
            } else if three_way(&to, &base.join(&rel), &from).is_some() {
                MemoryAction::Merge
            } else {
                MemoryAction::Conflict
            };
            MemoryFile { rel, action, newer }
        })
        .collect()
}

/// Merge `source` memory into `destination`, as [`plan_memory`] plans it,
/// with `decisions` for its conflicts, keyed by path. Destination files
/// that change are backed up under `backup_root`, at their path relative to
/// `account_dir`. `labels` name the two accounts in the report.
pub fn apply_memory(
    source: &Path,
    destination: &Path,
    base: &Path,
    account_dir: &Path,
    backup_root: &Path,
    decisions: &HashMap<String, Decision>,
    labels: (&str, &str),
) -> io::Result<MemoryReport> {
    let (source_label, destination_label) = labels;
    let plan = plan_memory(source, destination, base);
    if let Some(undecided) = plan
        .iter()
        .find(|file| file.action == MemoryAction::Conflict && !decisions.contains_key(&file.rel))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("memory/{} needs a decision", undecided.rel),
        ));
    }

    let mut report = MemoryReport::default();
    for file in plan {
        let rel = &file.rel;
        let (from, to) = (source.join(rel), destination.join(rel));
        let result: Option<Vec<u8>> = match file.action {
            MemoryAction::Add => {
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent)?;
                }
                copy_any(&from, &to)?;
                report.lines.push(format!("added    memory/{rel}"));
                None
            }
            MemoryAction::Same => None,
            MemoryAction::Index => {
                let (newer, older) = match file.newer {
                    Side::Destination => (&to, &from),
                    Side::Source => (&from, &to),
                };
                report
                    .lines
                    .push(format!("merged   memory/{rel}  (index lines from both)"));
                Some(
                    merge_index(&read_lossy(newer), &read_lossy(older), |name| {
                        destination.join(name).is_file()
                    })
                    .into_bytes(),
                )
            }
            MemoryAction::Merge => {
                report
                    .lines
                    .push(format!("merged   memory/{rel}  (changes from both)"));
                three_way(&to, &base.join(rel), &from)
            }
            MemoryAction::Conflict => match &decisions[rel] {
                Decision::Source => {
                    report
                        .lines
                        .push(format!("updated  memory/{rel}  (took {source_label}'s)"));
                    Some(fs::read(&from)?)
                }
                Decision::Destination => {
                    report
                        .lines
                        .push(format!("kept     memory/{rel}  ({destination_label}'s)"));
                    None
                }
                Decision::Merged(text) => {
                    report
                        .lines
                        .push(format!("merged   memory/{rel}  (by Claude)"));
                    Some(text.clone().into_bytes())
                }
            },
        };

        if let Some(result) = result {
            if fs::read(&to).ok().as_deref() != Some(result.as_slice()) {
                let backup = backup_root.join(to.strip_prefix(account_dir).unwrap_or(&to));
                if let Some(parent) = backup.parent() {
                    fs::create_dir_all(parent)?;
                }
                let _ = fs::remove_file(&backup);
                copy_any(&to, &backup)?;
                report.backed_up = true;
                write_into_place(&to, &result)?;
            }
        }

        let common = base.join(rel);
        if let Some(parent) = common.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&from, &common)?;
    }
    Ok(report)
}

/// Pure: the index merged the way claudemulti does. Every line of `newer`,
/// except those linking a note that no longer `exists`; then each line of
/// `older` linking a note that exists and `newer` doesn't link. Lines without
/// a link are kept from `newer` only.
pub fn merge_index(newer: &str, older: &str, exists: impl Fn(&str) -> bool) -> String {
    let mut result: Vec<&str> = Vec::new();
    let mut listed: Vec<&str> = Vec::new();
    for line in newer.lines() {
        match linked_note(line) {
            Some(name) if !exists(name) => continue,
            Some(name) => listed.push(name),
            None => {}
        }
        result.push(line);
    }
    for line in older.lines() {
        if let Some(name) = linked_note(line) {
            if !listed.contains(&name) && exists(name) {
                result.push(line);
                listed.push(name);
            }
        }
    }
    let mut merged = result.join("\n");
    merged.push('\n');
    merged
}

/// Pure: the first `](<name>.md)` link on `line`, its name without spaces or
/// `#`, as claudemulti's `\]\(([^)#\s]+\.md)\)` finds it.
pub fn linked_note(line: &str) -> Option<&str> {
    let mut rest = line;
    while let Some(at) = rest.find("](") {
        let after = &rest[at + 2..];
        if let Some(end) = after.find(')') {
            let name = &after[..end];
            if name.len() > 3
                && name.ends_with(".md")
                && !name.contains('#')
                && !name.chars().any(char::is_whitespace)
            {
                return Some(name);
            }
        }
        rest = after;
    }
    None
}

/// `git merge-file -p <current> <base> <other>`: the clean three-way merge,
/// or `None` when there's no base, it conflicts, or git can't be run.
pub fn three_way(current: &Path, base: &Path, other: &Path) -> Option<Vec<u8>> {
    if !base.is_file() {
        return None;
    }
    let output = Command::new("git")
        .args(["merge-file", "-p", "--"])
        .args([current, base, other])
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

/// Which of `source` and `destination` was written later; a tie counts as
/// the source.
pub fn newer_side(source: &Path, destination: &Path) -> Side {
    let modified = |path: &Path| fs::metadata(path).and_then(|meta| meta.modified()).ok();
    match (modified(source), modified(destination)) {
        (Some(from), Some(to)) if to > from => Side::Destination,
        _ => Side::Source,
    }
}

/// The text of a file, with anything that isn't UTF-8 replaced.
pub fn read_lossy(path: &Path) -> String {
    String::from_utf8_lossy(&fs::read(path).unwrap_or_default()).into_owned()
}

fn same_bytes(a: &Path, b: &Path) -> bool {
    matches!((fs::read(a), fs::read(b)), (Ok(a), Ok(b)) if a == b)
}

/// Where the common versions for the project folder `project` live, given
/// the accounts folder.
pub fn base_dir(accounts_base: &Path, project: &str) -> PathBuf {
    accounts_base.join(BASE_DIR).join(project)
}

/// Pure: claudemulti's prompt asking Claude to merge two versions of a
/// memory note, `a` (the destination's) and `b` (the source's).
pub fn claude_merge_prompt(
    newer_label: &str,
    a_label: &str,
    a: &str,
    b_label: &str,
    b: &str,
) -> String {
    format!(
        "Below are two versions of the same Claude Code memory note. Merge them into\n\
one note that keeps every distinct fact, rule and example from both. Remove\n\
repetition. Where they contradict each other, prefer the version from\n\
\"{newer_label}\", which is newer. Keep the note's format exactly, including any\n\
frontmatter between --- lines. Output only the merged file content: no\n\
commentary and no code fences.\n\
\n\
===== VERSION FROM \"{a_label}\" =====\n\
{a}\n\
===== VERSION FROM \"{b_label}\" =====\n\
{b}"
    )
}

/// Pure: Claude's merge as a file: trimmed of blank lines at either end, and
/// of a code fence around the whole of it, ending in one newline. `None` if
/// nothing is left.
pub fn clean_claude_merge(output: &str) -> Option<String> {
    let text = output.trim_matches('\n');
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.len() >= 2
        && lines[0].starts_with("```")
        && lines.last().is_some_and(|last| last.trim() == "```")
    {
        lines.remove(0);
        lines.pop();
    }
    let merged = lines.join("\n");
    (!merged.trim().is_empty()).then(|| format!("{merged}\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn git_available() -> bool {
        Command::new("git").arg("--version").output().is_ok()
    }

    #[test]
    fn finds_links_the_way_the_regex_does() {
        assert_eq!(linked_note("- [Build](build.md) — how"), Some("build.md"));
        assert_eq!(linked_note("- [a](http://x) then [b](b.md)"), Some("b.md"));
        assert_eq!(linked_note("- [a](a.md#part)"), None);
        assert_eq!(linked_note("- [a](my note.md)"), None);
        assert_eq!(linked_note("no link"), None);
    }

    #[test]
    fn the_index_keeps_the_newer_and_adds_what_only_the_older_links() {
        let newer = "# Memory\n- [A](a.md)\n- [Gone](gone.md)\n- [B](b.md)\n";
        let older = "# Old heading\n- [A](a.md) old wording\n- [C](c.md)\n- [D](d.md)\n";
        let exists = |name: &str| ["a.md", "b.md", "c.md"].contains(&name);
        assert_eq!(
            merge_index(newer, older, exists),
            "# Memory\n- [A](a.md)\n- [B](b.md)\n- [C](c.md)\n"
        );
    }

    #[test]
    fn lists_the_index_last() {
        let root = tempfile::tempdir().unwrap();
        for name in ["MEMORY.md", "b.md", "a.md", "sub/MEMORY.md"] {
            write(&root.path().join(name), "x");
        }
        assert_eq!(
            memory_files(root.path()),
            vec!["a.md", "b.md", "sub/MEMORY.md", "MEMORY.md"]
        );
    }

    #[test]
    fn plans_and_merges_file_by_file_backing_up_what_changes() {
        if !git_available() {
            eprintln!("skipped: git isn't installed");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let account = root.path().join("dst");
        let (src, dst) = (
            root.path().join("src/memory"),
            account.join("projects/-w/memory"),
        );
        let base = root.path().join("base");
        write(&src.join("new.md"), "only in the source\n");
        write(&src.join("same.md"), "same\n");
        write(&dst.join("same.md"), "same\n");
        // Changed on each side in a different place: merges cleanly.
        write(&base.join("clean.md"), "one\ntwo\nthree\nfour\n");
        write(&src.join("clean.md"), "ONE\ntwo\nthree\nfour\n");
        write(&dst.join("clean.md"), "one\ntwo\nthree\nFOUR\n");
        // Changed on both sides in the same place: a conflict.
        write(&base.join("clash.md"), "rule\n");
        write(&src.join("clash.md"), "rule from source\n");
        write(&dst.join("clash.md"), "rule from destination\n");
        // No common version at all: a conflict too.
        write(&src.join("nobase.md"), "s\n");
        write(&dst.join("nobase.md"), "d\n");
        write(&dst.join("only-dst.md"), "untouched\n");
        write(
            &src.join("MEMORY.md"),
            "- [New](new.md)\n- [Clean](clean.md)\n",
        );
        write(&dst.join("MEMORY.md"), "- [Only](only-dst.md)\n");

        let plan = plan_memory(&src, &dst, &base);
        let actions: Vec<(&str, MemoryAction)> = plan
            .iter()
            .map(|file| (file.rel.as_str(), file.action))
            .collect();
        assert_eq!(
            actions,
            vec![
                ("clash.md", MemoryAction::Conflict),
                ("clean.md", MemoryAction::Merge),
                ("new.md", MemoryAction::Add),
                ("nobase.md", MemoryAction::Conflict),
                ("same.md", MemoryAction::Same),
                ("MEMORY.md", MemoryAction::Index),
            ]
        );

        let backup = account.join("session-transfer-backups/s/1");
        let partial = HashMap::from([("clash.md".to_string(), Decision::Source)]);
        assert!(apply_memory(&src, &dst, &base, &account, &backup, &partial, ("a", "b")).is_err());

        let decisions = HashMap::from([
            ("clash.md".to_string(), Decision::Source),
            (
                "nobase.md".to_string(),
                Decision::Merged("both\n".to_string()),
            ),
        ]);
        let report =
            apply_memory(&src, &dst, &base, &account, &backup, &decisions, ("a", "b")).unwrap();
        assert!(report.backed_up);
        assert_eq!(
            fs::read_to_string(dst.join("clean.md")).unwrap(),
            "ONE\ntwo\nthree\nFOUR\n"
        );
        assert_eq!(
            fs::read_to_string(dst.join("clash.md")).unwrap(),
            "rule from source\n"
        );
        assert_eq!(fs::read_to_string(dst.join("nobase.md")).unwrap(), "both\n");
        assert_eq!(
            fs::read_to_string(dst.join("new.md")).unwrap(),
            "only in the source\n"
        );
        assert_eq!(
            fs::read_to_string(dst.join("only-dst.md")).unwrap(),
            "untouched\n"
        );
        let index = fs::read_to_string(dst.join("MEMORY.md")).unwrap();
        assert!(
            index.contains("new.md") && index.contains("only-dst.md"),
            "{index}"
        );
        assert_eq!(
            fs::read_to_string(backup.join("projects/-w/memory/clash.md")).unwrap(),
            "rule from destination\n"
        );
        assert_eq!(
            fs::read_to_string(base.join("clash.md")).unwrap(),
            "rule from source\n"
        );
        assert!(report.lines.iter().any(|line| line.contains("(took a's)")));
    }

    #[test]
    fn cleans_a_fenced_claude_merge() {
        assert_eq!(
            clean_claude_merge("\n```md\nkeep\n```\n").as_deref(),
            Some("keep\n")
        );
        assert_eq!(clean_claude_merge("plain\n\n").as_deref(), Some("plain\n"));
        assert_eq!(clean_claude_merge("\n\n"), None);
    }

    #[test]
    fn asks_claude_with_the_newer_version_named() {
        let prompt = claude_merge_prompt("work", "work", "A", "home", "B");
        assert!(prompt.contains("prefer the version from\n\"work\", which is newer"));
        assert!(prompt.ends_with(
            "===== VERSION FROM \"work\" =====\nA\n===== VERSION FROM \"home\" =====\nB"
        ));
    }
}

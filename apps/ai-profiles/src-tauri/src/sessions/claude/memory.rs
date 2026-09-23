//! Bringing a project's memory along when a session moves.
//!
//! Claude Code keeps what it learns about a project in
//! `<config>/projects/<slug>/memory/`: one Markdown file per memory and a
//! `MEMORY.md` index linking to each (`- [Title](file.md) — hook`). The
//! destination may have memories of the same project already, which a move
//! must not lose.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use super::copy::{compare, place, write_replacing, ItemAction};
use crate::error::AppResult;

/// The index of a memory folder.
const INDEX: &str = "MEMORY.md";

/// Merge the memory folder `from` into `to`: files `to` lacks are copied
/// and their `MEMORY.md` index lines appended to its index (the whole index
/// is copied when `to` has none); files that differ on both sides stay as
/// `to` has them. An index that is rewritten is backed up to `backup` first.
/// Returns the names of the files that differ, the conflicts.
pub fn merge_memory(from: &Path, to: &Path, backup: &Path) -> AppResult<Vec<String>> {
    let Ok(entries) = fs::read_dir(from) else {
        return Ok(Vec::new());
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name != INDEX && !name.starts_with('.'))
        .collect();
    names.sort();
    let mut copied = HashSet::new();
    let mut conflicts = Vec::new();
    for name in names {
        let relative = Path::new(&name);
        match compare(&from.join(&name), &to.join(&name))? {
            ItemAction::Copy => {
                place(&from.join(&name), to, relative, backup)?;
                copied.insert(name);
            }
            ItemAction::Same => {}
            ItemAction::Replace => conflicts.push(name),
        }
    }
    merge_index(from, to, backup, &copied)?;
    Ok(conflicts)
}

/// Whether [`merge_memory`] of `from` into `to` would write anything: `to`
/// lacks one of `from`'s files, its index included.
pub fn merge_writes(from: &Path, to: &Path) -> bool {
    let Ok(entries) = fs::read_dir(from) else {
        return false;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .any(|entry| fs::symlink_metadata(to.join(entry.file_name())).is_err())
}

/// Bring `from`'s index into `to`: whole when `to` has none, else only the
/// lines linking to the files just `copied` that `to`'s index lacks.
fn merge_index(from: &Path, to: &Path, backup: &Path, copied: &HashSet<String>) -> AppResult<()> {
    let Ok(theirs) = fs::read_to_string(from.join(INDEX)) else {
        return Ok(());
    };
    let Ok(ours) = fs::read_to_string(to.join(INDEX)) else {
        return place(&from.join(INDEX), to, Path::new(INDEX), backup);
    };
    let have: HashSet<&str> = ours.lines().map(str::trim_end).collect();
    let added: Vec<&str> = theirs
        .lines()
        .map(str::trim_end)
        .filter(|line| !have.contains(line))
        .filter(|line| linked_file(line).is_some_and(|file| copied.contains(file)))
        .collect();
    if added.is_empty() {
        return Ok(());
    }
    let mut merged = ours.clone();
    if !merged.is_empty() && !merged.ends_with('\n') {
        merged.push('\n');
    }
    for line in added {
        merged.push_str(line);
        merged.push('\n');
    }
    write_replacing(&merged, to, Path::new(INDEX), backup)
}

/// The file an index line links to: `file.md` of `- [Title](file.md) — hook`.
fn linked_file(line: &str) -> Option<&str> {
    let start = line.find("](")? + 2;
    let end = start + line[start..].find(')')?;
    Some(&line[start..end])
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    /// Write `contents` to `path`, making its folder.
    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    /// The file at `path`, as text.
    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn missing_memories_are_copied_and_indexed_and_differing_ones_are_conflicts() {
        let root = tempdir().unwrap();
        let from = root.path().join("from/memory");
        let to = root.path().join("to/memory");
        let backup = root.path().join("backup");
        write(
            &from.join("MEMORY.md"),
            "- [Style](style.md) — tabs\n- [Stack](stack.md) — rust\n- [Deploy](deploy.md) — vercel\n",
        );
        write(&from.join("style.md"), "Use tabs");
        write(&from.join("stack.md"), "Rust");
        write(&from.join("deploy.md"), "Vercel");
        write(
            &to.join("MEMORY.md"),
            "# Memory\n- [Stack](stack.md) — rust\n",
        );
        write(&to.join("stack.md"), "Rust");
        write(&to.join("deploy.md"), "Netlify");

        let conflicts = merge_memory(&from, &to, &backup).unwrap();

        assert_eq!(conflicts, ["deploy.md"]);
        assert_eq!(read(&to.join("style.md")), "Use tabs");
        assert_eq!(read(&to.join("deploy.md")), "Netlify");
        assert_eq!(
            read(&to.join("MEMORY.md")),
            "# Memory\n- [Stack](stack.md) — rust\n- [Style](style.md) — tabs\n"
        );
        assert_eq!(
            read(&backup.join("MEMORY.md")),
            "# Memory\n- [Stack](stack.md) — rust\n"
        );
    }

    #[test]
    fn a_destination_without_memory_gets_all_of_it() {
        let root = tempdir().unwrap();
        let from = root.path().join("from/memory");
        let to = root.path().join("to/memory");
        let backup = root.path().join("backup");
        write(&from.join("MEMORY.md"), "- [Style](style.md) — tabs\n");
        write(&from.join("style.md"), "Use tabs");

        let conflicts = merge_memory(&from, &to, &backup).unwrap();

        assert_eq!(conflicts, Vec::<String>::new());
        assert_eq!(read(&to.join("MEMORY.md")), "- [Style](style.md) — tabs\n");
        assert_eq!(read(&to.join("style.md")), "Use tabs");
        assert!(!backup.exists());
    }

    #[test]
    fn an_index_with_nothing_to_add_is_left_alone() {
        let root = tempdir().unwrap();
        let from = root.path().join("from/memory");
        let to = root.path().join("to/memory");
        let backup = root.path().join("backup");
        write(&from.join("MEMORY.md"), "- [Stack](stack.md) — rust\n");
        write(&from.join("stack.md"), "Rust");
        write(&to.join("MEMORY.md"), "- [Stack](stack.md) — rust");
        write(&to.join("stack.md"), "Rust");

        merge_memory(&from, &to, &backup).unwrap();
        merge_memory(&root.path().join("nowhere"), &to, &backup).unwrap();

        assert_eq!(read(&to.join("MEMORY.md")), "- [Stack](stack.md) — rust");
        assert!(!backup.exists());
    }

    #[test]
    fn a_merge_writes_only_when_the_destination_lacks_a_memory() {
        let root = tempdir().unwrap();
        let from = root.path().join("from/memory");
        let to = root.path().join("to/memory");
        write(&from.join("MEMORY.md"), "- [Stack](stack.md) — rust\n");
        write(&from.join("stack.md"), "Rust");
        write(&to.join("stack.md"), "Go");

        assert!(merge_writes(&from, &to));

        write(&to.join("MEMORY.md"), "");

        assert!(!merge_writes(&from, &to));
        assert!(!merge_writes(&root.path().join("nowhere"), &to));
    }
}

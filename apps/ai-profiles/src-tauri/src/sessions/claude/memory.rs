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

use super::copy::{place, same_bytes, write_replacing};
use crate::error::AppResult;
use crate::sessions::fs_ops::occupied;

/// The index of a memory folder.
const INDEX: &str = "MEMORY.md";

/// A file a memory merge put in the destination folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryWrite {
    /// Its name in the folder.
    pub name: String,
    /// It replaced a file there, which is in the backup folder now, under the
    /// same name.
    pub replaced: bool,
}

/// Merge the memory folder `from` into `to`: files `to` lacks are copied
/// and their `MEMORY.md` index lines appended to its index (the whole index
/// is copied when `to` has none); files that differ on both sides stay as
/// `to` has them. An index that is rewritten is backed up to `backup` first.
/// Each file put in `to` is logged in `written` as soon as it is there, so a
/// merge that fails part way says what it did. Returns the names of the files
/// that differ, the conflicts.
pub fn merge_memory(
    from: &Path,
    to: &Path,
    backup: &Path,
    written: &mut Vec<MemoryWrite>,
) -> AppResult<Vec<String>> {
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
        let (theirs, ours) = (from.join(&name), to.join(&name));
        if !occupied(&ours) {
            let replaced = place(&theirs, to, Path::new(&name), backup)?;
            written.push(MemoryWrite {
                name: name.clone(),
                replaced,
            });
            copied.insert(name);
        } else if !same_bytes(&theirs, &ours)? {
            // Memories are small and often written apart with the same
            // words, so they are told apart by their bytes alone.
            conflicts.push(name);
        }
    }
    merge_index(from, to, backup, &copied, written)?;
    Ok(conflicts)
}

/// The files [`merge_memory`] of `from` into `to` would copy: those of
/// `from`'s that `to` lacks, its index included, by name, sorted. It writes
/// nothing when there are none.
pub fn merge_copies(from: &Path, to: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(from) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| !name.starts_with('.') && !occupied(&to.join(name)))
        .collect();
    names.sort();
    names
}

/// Bring `from`'s index into `to`: whole when `to` has none, else only the
/// lines linking to the files just `copied` that `to`'s index lacks. Logs the
/// index in `written` if it is written.
fn merge_index(
    from: &Path,
    to: &Path,
    backup: &Path,
    copied: &HashSet<String>,
    written: &mut Vec<MemoryWrite>,
) -> AppResult<()> {
    let Ok(theirs) = fs::read_to_string(from.join(INDEX)) else {
        return Ok(());
    };
    let Ok(ours) = fs::read_to_string(to.join(INDEX)) else {
        let replaced = place(&from.join(INDEX), to, Path::new(INDEX), backup)?;
        written.push(MemoryWrite {
            name: INDEX.to_string(),
            replaced,
        });
        return Ok(());
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
    let replaced = write_replacing(&merged, to, Path::new(INDEX), backup)?;
    written.push(MemoryWrite {
        name: INDEX.to_string(),
        replaced,
    });
    Ok(())
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

        let mut written = Vec::new();
        let conflicts = merge_memory(&from, &to, &backup, &mut written).unwrap();

        assert_eq!(conflicts, ["deploy.md"]);
        assert_eq!(
            written,
            [
                MemoryWrite {
                    name: "style.md".to_string(),
                    replaced: false,
                },
                MemoryWrite {
                    name: "MEMORY.md".to_string(),
                    replaced: true,
                },
            ]
        );
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

        let conflicts = merge_memory(&from, &to, &backup, &mut Vec::new()).unwrap();

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

        merge_memory(&from, &to, &backup, &mut Vec::new()).unwrap();
        merge_memory(&root.path().join("nowhere"), &to, &backup, &mut Vec::new()).unwrap();

        assert_eq!(read(&to.join("MEMORY.md")), "- [Stack](stack.md) — rust");
        assert!(!backup.exists());
    }

    #[test]
    fn a_merge_copies_only_what_the_destination_lacks() {
        let root = tempdir().unwrap();
        let from = root.path().join("from/memory");
        let to = root.path().join("to/memory");
        write(&from.join("MEMORY.md"), "- [Stack](stack.md) — rust\n");
        write(&from.join("stack.md"), "Rust");
        write(&from.join("style.md"), "Tabs");
        write(&to.join("stack.md"), "Go");

        assert_eq!(merge_copies(&from, &to), ["MEMORY.md", "style.md"]);

        write(&to.join("MEMORY.md"), "");
        write(&to.join("style.md"), "Spaces");

        assert_eq!(merge_copies(&from, &to), Vec::<String>::new());
        assert_eq!(
            merge_copies(&root.path().join("nowhere"), &to),
            Vec::<String>::new()
        );
    }
}

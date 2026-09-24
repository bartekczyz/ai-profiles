//! Moving a session between two Claude config directories, and archiving
//! one, the way claudemulti does.
//!
//! A session is more than its transcript. A move copies everything stored
//! under its id, which no other session in the destination can be using: the
//! session folder beside the transcript (subagents, tool results),
//! `file-history/<id>` (what `/rewind` restores), `session-env/<id>`,
//! `tasks/<id>`, `todos/<id>-*.json`, the plans the transcript names, and
//! the transcript itself, last. Each item is copied under a temporary name
//! and renamed into place. Whatever a move replaces or removes in the
//! destination is backed up first, to
//! `<destination>/session-transfer-backups/<id>/<stamp>/`, at its path in
//! the account. Project memory is merged, not copied: see [`crate::memory`].
//!
//! Archiving moves only the transcript, to
//! `<account>/session-transfer-backups/<id>/<stamp>-archived/`, at its path
//! in the account; the session's other files stay, since the transcript
//! names them by absolute path.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::transcript::is_safe_name;

/// Where backups and archives go, in each account.
pub const BACKUPS_DIR: &str = "session-transfer-backups";

/// The end of an archive folder's name: `<stamp>-archived`.
pub const ARCHIVED_SUFFIX: &str = "-archived";

/// What a move does with one item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemAction {
    /// Not in the destination yet.
    Copy,
    /// Already there, identical.
    Same,
    /// There, different: backed up, then replaced.
    Replace,
    /// Only the destination has it, left over from an earlier copy of the
    /// session: backed up, then removed.
    Remove,
}

/// One thing a move copies, replaces or removes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub from: PathBuf,
    pub to: PathBuf,
    /// Relative to the destination account.
    pub rel: PathBuf,
    pub action: ItemAction,
}

/// Why a move can't be planned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MoveError {
    /// The destination has more than one copy of the transcript, none at the
    /// source's path: which to replace isn't a guess to make. Their paths.
    AmbiguousDestination(Vec<PathBuf>),
    Io(String),
}

impl std::fmt::Display for MoveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MoveError::AmbiguousDestination(paths) => write!(
                formatter,
                "The destination has {} copies of this session ({}). Refusing to guess which should be replaced.",
                paths.len(),
                paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            MoveError::Io(message) => formatter.write_str(message),
        }
    }
}

impl From<io::Error> for MoveError {
    fn from(err: io::Error) -> MoveError {
        MoveError::Io(err.to_string())
    }
}

/// Every transcript of session `id` in the account at `account_dir`:
/// `projects/<project>/<id>.jsonl`.
pub fn transcripts_of(account_dir: &Path, id: &str) -> Vec<PathBuf> {
    let Ok(projects) = fs::read_dir(account_dir.join("projects")) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = projects
        .flatten()
        .map(|project| project.path().join(format!("{id}.jsonl")))
        .filter(|path| fs::symlink_metadata(path).is_ok_and(|meta| meta.is_file()))
        .collect();
    found.sort();
    found
}

/// Where session `id`, whose transcript is `source_transcript` in the account
/// at `source_dir`, goes in the account at `destination_dir`: the copy the
/// destination already has, wherever it is, or the source's path otherwise.
/// With several copies, the one at the source's path if there is one.
pub fn destination_transcript(
    source_dir: &Path,
    source_transcript: &Path,
    destination_dir: &Path,
    id: &str,
) -> Result<PathBuf, MoveError> {
    let default = destination_dir.join(
        source_transcript
            .strip_prefix(source_dir)
            .map_err(|_| MoveError::Io("the transcript isn't in its account".into()))?,
    );
    let mut existing = transcripts_of(destination_dir, id);
    match existing.len() {
        0 => Ok(default),
        1 => Ok(existing.remove(0)),
        _ if default.is_file() => Ok(default),
        _ => Err(MoveError::AmbiguousDestination(existing)),
    }
}

/// A plan slug the move may copy: `plans/<slug>.md` stays inside `plans`.
fn is_plan_slug(slug: &str) -> bool {
    !slug.is_empty()
        && !slug.starts_with('.')
        && slug
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Everything moving session `id` from `source_dir` to `destination_dir`
/// touches, in the order it's done, transcript last. `slugs` are the plan
/// slugs the source transcript names.
pub fn plan_items<'a>(
    source_dir: &Path,
    destination_dir: &Path,
    id: &str,
    source_transcript: &Path,
    destination_transcript: &Path,
    slugs: impl IntoIterator<Item = &'a String>,
) -> io::Result<Vec<Item>> {
    let rel_to = |path: &Path| {
        path.strip_prefix(destination_dir)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let source_project = source_transcript.parent().unwrap_or(source_dir);
    let destination_project = destination_transcript.parent().unwrap_or(destination_dir);

    // (from, to), both absolute.
    let mut pairs: Vec<(PathBuf, PathBuf)> =
        vec![(source_project.join(id), destination_project.join(id))];
    for state in ["file-history", "session-env", "tasks"] {
        pairs.push((
            source_dir.join(state).join(id),
            destination_dir.join(state).join(id),
        ));
    }
    let prefix = format!("{id}-");
    let mut todos: Vec<String> = [source_dir, destination_dir]
        .iter()
        .filter_map(|dir| fs::read_dir(dir.join("todos")).ok())
        .flat_map(|entries| entries.flatten())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(&prefix) && name.ends_with(".json"))
        .collect();
    todos.sort();
    todos.dedup();
    for name in todos {
        pairs.push((
            source_dir.join("todos").join(&name),
            destination_dir.join("todos").join(&name),
        ));
    }
    let mut slugs: Vec<&String> = slugs
        .into_iter()
        .filter(|slug| is_plan_slug(slug))
        .collect();
    slugs.sort();
    slugs.dedup();
    for slug in slugs {
        let from = source_dir.join("plans").join(format!("{slug}.md"));
        if from.is_file() {
            pairs.push((
                from,
                destination_dir.join("plans").join(format!("{slug}.md")),
            ));
        }
    }
    pairs.push((
        source_transcript.to_path_buf(),
        destination_transcript.to_path_buf(),
    ));

    let mut items = Vec::new();
    for (from, to) in pairs {
        let (has_from, has_to) = (exists(&from), exists(&to));
        let action = match (has_from, has_to) {
            (true, false) => ItemAction::Copy,
            (true, true) if identical(&from, &to)? => ItemAction::Same,
            (true, true) => ItemAction::Replace,
            (false, true) => ItemAction::Remove,
            (false, false) => continue,
        };
        items.push(Item {
            rel: rel_to(&to),
            from,
            to,
            action,
        });
    }
    Ok(items)
}

/// Whether the destination's copy of the transcript differs from the
/// source's and was written later, to the second: a move would roll it back.
pub fn destination_newer(source_transcript: &Path, destination_transcript: &Path) -> bool {
    if !destination_transcript.is_file()
        || identical(source_transcript, destination_transcript).unwrap_or(true)
    {
        return false;
    }
    mtime_secs(destination_transcript) > mtime_secs(source_transcript)
}

/// A file's modification time in whole seconds.
pub fn mtime_secs(path: &Path) -> u64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|time| time.as_secs())
        .unwrap_or(0)
}

/// Carry out `items`: first back up, under `backup_root`, everything that
/// will be replaced or removed, then apply them in order. `true` if
/// anything was backed up.
pub fn apply_items(items: &[Item], backup_root: &Path) -> io::Result<bool> {
    let mut backed_up = false;
    for item in items {
        if matches!(item.action, ItemAction::Replace | ItemAction::Remove) {
            let backup = backup_root.join(&item.rel);
            if let Some(parent) = backup.parent() {
                fs::create_dir_all(parent)?;
            }
            remove_any(&backup)?;
            copy_any(&item.to, &backup)?;
            backed_up = true;
        }
    }
    for item in items {
        match item.action {
            ItemAction::Copy | ItemAction::Replace => replace_item(&item.from, &item.to)?,
            ItemAction::Remove => remove_any(&item.to)?,
            ItemAction::Same => {}
        }
    }
    Ok(backed_up)
}

/// `<account_dir>/session-transfer-backups/<id>/<stamp>`.
pub fn backup_root(account_dir: &Path, id: &str, stamp: &str) -> PathBuf {
    account_dir.join(BACKUPS_DIR).join(id).join(stamp)
}

/// Move the transcript at `transcript` into an archive of its session, and
/// say where it went.
pub fn archive_transcript(
    account_dir: &Path,
    transcript: &Path,
    stamp: &str,
) -> io::Result<PathBuf> {
    let id = transcript
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let rel = transcript
        .strip_prefix(account_dir)
        .map_err(|_| io::Error::other("the transcript isn't in its account"))?;
    let archived = account_dir
        .join(BACKUPS_DIR)
        .join(&id)
        .join(format!("{stamp}{ARCHIVED_SUFFIX}"))
        .join(rel);
    if let Some(parent) = archived.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(transcript, &archived)?;
    // Moved first, compressed after: should compressing fail, the archive is
    // still whole, only bigger.
    Ok(compress(&archived).unwrap_or(archived))
}

/// Gzip the file at `path` into `<path>.gz` beside it, keeping its modified
/// time, then remove it. Returns where it is now. Transcripts are mostly
/// repeated JSON, and shrink several times over.
pub fn compress(path: &Path) -> io::Result<PathBuf> {
    let compressed = with_suffix(path, ".gz");
    let modified = fs::metadata(path)?.modified()?;
    write_via_part(&compressed, modified, |out| {
        let mut encoder = flate2::write::GzEncoder::new(out, flate2::Compression::default());
        io::copy(&mut fs::File::open(path)?, &mut encoder)?;
        encoder.finish()
    })?;
    // Gone meanwhile (restored as it was being compressed): then so is its
    // compressed copy, or it would still be listed as archived.
    if let Err(err) = fs::remove_file(path) {
        let _ = fs::remove_file(&compressed);
        return Err(err);
    }
    Ok(compressed)
}

/// Unzip `compressed` to `to`, making `to`'s folder and keeping the modified
/// time, then remove it.
pub fn decompress(compressed: &Path, to: &Path) -> io::Result<()> {
    let modified = fs::metadata(compressed)?.modified()?;
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    write_via_part(to, modified, |mut out| {
        let mut decoder = flate2::read::GzDecoder::new(fs::File::open(compressed)?);
        io::copy(&mut decoder, &mut out)?;
        Ok(out)
    })?;
    fs::remove_file(compressed)
}

/// Write `to` through `write`, into `<to>.part` first, only its user able to
/// read it, and renamed into place once it's all on disk with `modified` as
/// its modified time: `to` is never there half-written.
fn write_via_part(
    to: &Path,
    modified: std::time::SystemTime,
    write: impl FnOnce(io::BufWriter<fs::File>) -> io::Result<io::BufWriter<fs::File>>,
) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let part = with_suffix(to, ".part");
    let result = (|| {
        let file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&part)?;
        let file = write(io::BufWriter::new(file))?
            .into_inner()
            .map_err(io::IntoInnerError::into_error)?;
        file.set_modified(modified)?;
        file.sync_all()?;
        fs::rename(&part, to)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

/// Whether `path` is a gzipped archive, `<id>.jsonl.gz`.
pub fn is_compressed(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "gz")
}

/// `path` with `suffix` added to its name.
pub fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Compress each archived transcript of the account at `account_dir` that
/// isn't yet: those archived before archives were. Returns each one's path
/// with its size before and after.
pub fn compress_archived(account_dir: &Path) -> Vec<(PathBuf, io::Result<(u64, u64)>)> {
    list_archived(account_dir)
        .into_iter()
        .filter(|archived| archived.path.extension().is_some_and(|ext| ext == "jsonl"))
        .map(|archived| {
            let before = size_of(&archived.path);
            let outcome = compress(&archived.path).map(|now| (before, size_of(&now)));
            (archived.path, outcome)
        })
        .collect()
}

/// Whether `item` is the session's own, so its source copy can go once the
/// session has moved: everything but plan files, which other sessions may
/// name too.
fn session_own(item: &Item) -> bool {
    !item.rel.starts_with("plans")
}

/// What a file, folder or link takes, as the sum of its files' lengths.
pub fn size_of(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    fs::read_dir(path)
        .map(|entries| entries.flatten().map(|entry| size_of(&entry.path())).sum())
        .unwrap_or(0)
}

/// What deleting the source's copy of the session would free, after moving
/// `items`.
pub fn source_bytes(items: &[Item]) -> u64 {
    items
        .iter()
        .filter(|item| session_own(item))
        .map(|item| size_of(&item.from))
        .sum()
}

/// After `items` moved, delete the source's copy of the session: each of its
/// own items, once every one of them is checked to be identical at its
/// destination. Nothing is deleted unless all are. Returns what it freed.
pub fn delete_source(items: &[Item]) -> io::Result<u64> {
    let own: Vec<&Item> = items
        .iter()
        .filter(|item| session_own(item) && exists(&item.from))
        .collect();
    for item in &own {
        if same_file(&item.from, &item.to) {
            return Err(io::Error::other(format!(
                "{} and the moved copy are the same file",
                item.rel.display()
            )));
        }
        if !exists(&item.to) || !identical(&item.from, &item.to)? {
            return Err(io::Error::other(format!(
                "{} isn't the same as the moved copy",
                item.rel.display()
            )));
        }
    }
    let freed = own.iter().map(|item| size_of(&item.from)).sum();
    for item in own {
        remove_any(&item.from)?;
    }
    Ok(freed)
}

/// One archived transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Archived {
    pub id: String,
    /// `<stamp>-archived`.
    pub archive: String,
    /// `<stamp>`, `%Y%m%d-%H%M%S` in the host's local time.
    pub stamp: String,
    /// Where it is now: `<id>.jsonl.gz`, or `<id>.jsonl` from before archives
    /// were compressed.
    pub path: PathBuf,
    /// Where it goes back to.
    pub original: PathBuf,
}

/// Every archived transcript in the account at `account_dir`.
pub fn list_archived(account_dir: &Path) -> Vec<Archived> {
    let mut found = Vec::new();
    let Ok(sessions) = fs::read_dir(account_dir.join(BACKUPS_DIR)) else {
        return found;
    };
    for session in sessions.flatten() {
        let id = session.file_name().to_string_lossy().into_owned();
        if !is_safe_name(&id) {
            continue;
        }
        let Ok(archives) = fs::read_dir(session.path()) else {
            continue;
        };
        for archive in archives.flatten() {
            let name = archive.file_name().to_string_lossy().into_owned();
            let Some(stamp) = name.strip_suffix(ARCHIVED_SUFFIX) else {
                continue;
            };
            let Ok(projects) = fs::read_dir(archive.path().join("projects")) else {
                continue;
            };
            for project in projects.flatten() {
                // Compressed, or not yet (archived before archives were, or
                // compressing it failed). Both only while a compression was
                // cut short: the plain one is then whole.
                let plain = project.path().join(format!("{id}.jsonl"));
                let compressed = with_suffix(&plain, ".gz");
                let path = if plain.is_file() {
                    plain
                } else if compressed.is_file() {
                    compressed
                } else {
                    continue;
                };
                found.push(Archived {
                    original: account_dir
                        .join("projects")
                        .join(project.file_name())
                        .join(format!("{id}.jsonl")),
                    id: id.clone(),
                    archive: name.clone(),
                    stamp: stamp.to_owned(),
                    path,
                });
            }
        }
    }
    found.sort_by(|a, b| b.stamp.cmp(&a.stamp).then(a.id.cmp(&b.id)));
    found
}

/// Put `archived` back where it came from, and remove the archive folder if
/// that left it empty. Refuses while the account has a live copy of the
/// session.
pub fn restore(account_dir: &Path, archived: &Archived) -> io::Result<PathBuf> {
    if !transcripts_of(account_dir, &archived.id).is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "The account already has this session. Archive or move that copy first.",
        ));
    }
    if let Some(parent) = archived.original.parent() {
        fs::create_dir_all(parent)?;
    }
    if is_compressed(&archived.path) {
        decompress(&archived.path, &archived.original)?;
    } else {
        fs::rename(&archived.path, &archived.original)?;
    }
    let root = account_dir
        .join(BACKUPS_DIR)
        .join(&archived.id)
        .join(&archived.archive);
    remove_empty_dirs(&root);
    Ok(archived.original.clone())
}

/// Remove `dir` and every folder under it that holds nothing but empty
/// folders.
fn remove_empty_dirs(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                remove_empty_dirs(&entry.path());
            }
        }
    }
    let _ = fs::remove_dir(dir);
}

/// Whether `a` and `b` are one file by two names (a link, or a folder
/// linked twice): deleting one would delete both.
fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::symlink_metadata(a), fs::symlink_metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

fn exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Whether two files, folders or links hold the same thing: same kind, same
/// bytes, same link targets, same names in a folder, recursively.
pub fn identical(a: &Path, b: &Path) -> io::Result<bool> {
    let (meta_a, meta_b) = (fs::symlink_metadata(a)?, fs::symlink_metadata(b)?);
    let (kind_a, kind_b) = (meta_a.file_type(), meta_b.file_type());
    if kind_a.is_symlink() || kind_b.is_symlink() {
        return Ok(kind_a.is_symlink()
            && kind_b.is_symlink()
            && fs::read_link(a)? == fs::read_link(b)?);
    }
    if kind_a.is_file() && kind_b.is_file() {
        return Ok(meta_a.len() == meta_b.len() && fs::read(a)? == fs::read(b)?);
    }
    if kind_a.is_dir() && kind_b.is_dir() {
        let names = |dir: &Path| -> io::Result<Vec<std::ffi::OsString>> {
            let mut names: Vec<_> = fs::read_dir(dir)?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect::<Result<_, _>>()?;
            names.sort();
            Ok(names)
        };
        let (names_a, names_b) = (names(a)?, names(b)?);
        if names_a != names_b {
            return Ok(false);
        }
        for name in names_a {
            if !identical(&a.join(&name), &b.join(&name))? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    Ok(false)
}

/// The temporary name `path` is built under: never `.jsonl`, so Claude
/// doesn't list it.
fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    path.with_file_name(format!("{name}.ai-profiles-tmp.{}", std::process::id()))
}

/// Put a copy of `from` at `to`, replacing whatever is there, by way of a
/// temporary name beside it.
pub fn replace_item(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = temp_path(to);
    remove_any(&tmp)?;
    copy_any(from, &tmp)?;
    if fs::symlink_metadata(to).is_ok_and(|meta| meta.is_dir()) {
        fs::remove_dir_all(to)?;
    }
    fs::rename(&tmp, to)
}

/// Write `bytes` to `to` by way of a temporary name beside it.
pub fn write_into_place(to: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = temp_path(to);
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, to)
}

/// Copy a file, folder or link, keeping links as links.
pub fn copy_any(from: &Path, to: &Path) -> io::Result<()> {
    let kind = fs::symlink_metadata(from)?.file_type();
    if kind.is_symlink() {
        std::os::unix::fs::symlink(fs::read_link(from)?, to)?;
    } else if kind.is_dir() {
        fs::create_dir(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            copy_any(&entry.path(), &to.join(entry.file_name()))?;
        }
    } else {
        fs::copy(from, to)?;
    }
    Ok(())
}

fn remove_any(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "11111111-2222-3333-4444-555555555555";

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn actions(items: &[Item]) -> Vec<(String, ItemAction)> {
        items
            .iter()
            .map(|item| (item.rel.display().to_string(), item.action))
            .collect()
    }

    #[test]
    fn plans_every_item_of_the_session_transcript_last() {
        let root = tempfile::tempdir().unwrap();
        let (src, dst) = (root.path().join("a"), root.path().join("b"));
        let transcript = src.join(format!("projects/-w/{ID}.jsonl"));
        write(&transcript, "t\n");
        write(
            &src.join(format!("projects/-w/{ID}/subagents/x.jsonl")),
            "s",
        );
        write(&src.join(format!("file-history/{ID}/f")), "f");
        write(&src.join(format!("todos/{ID}-agent-1.json")), "[]");
        write(&dst.join(format!("todos/{ID}-agent-2.json")), "[]");
        write(&dst.join(format!("tasks/{ID}/t")), "old");
        write(&src.join("plans/bold-plan.md"), "plan");
        write(&src.join("plans/unnamed.md"), "not named by the transcript");
        write(&dst.join(format!("todos/{}-agent-1.json", "other")), "[]");
        let to = destination_transcript(&src, &transcript, &dst, ID).unwrap();
        let slugs = [
            "bold-plan".to_string(),
            "../escape".to_string(),
            "missing".to_string(),
        ];
        let items = plan_items(&src, &dst, ID, &transcript, &to, &slugs).unwrap();
        assert_eq!(
            actions(&items),
            vec![
                (format!("projects/-w/{ID}"), ItemAction::Copy),
                (format!("file-history/{ID}"), ItemAction::Copy),
                (format!("tasks/{ID}"), ItemAction::Remove),
                (format!("todos/{ID}-agent-1.json"), ItemAction::Copy),
                (format!("todos/{ID}-agent-2.json"), ItemAction::Remove),
                ("plans/bold-plan.md".to_string(), ItemAction::Copy),
                (format!("projects/-w/{ID}.jsonl"), ItemAction::Copy),
            ]
        );
    }

    #[test]
    fn applies_after_backing_up_what_it_replaces_or_removes() {
        let root = tempfile::tempdir().unwrap();
        let (src, dst) = (root.path().join("a"), root.path().join("b"));
        let transcript = src.join(format!("projects/-w/{ID}.jsonl"));
        write(&transcript, "new\n");
        write(&dst.join(format!("projects/-w/{ID}.jsonl")), "old\n");
        write(&dst.join(format!("session-env/{ID}/e")), "left over");
        let to = destination_transcript(&src, &transcript, &dst, ID).unwrap();
        let items = plan_items(&src, &dst, ID, &transcript, &to, &[]).unwrap();
        let backup = backup_root(&dst, ID, "20260101-000000");
        assert!(apply_items(&items, &backup).unwrap());
        assert_eq!(fs::read_to_string(&to).unwrap(), "new\n");
        assert!(!dst.join(format!("session-env/{ID}")).exists());
        assert_eq!(
            fs::read_to_string(backup.join(format!("projects/-w/{ID}.jsonl"))).unwrap(),
            "old\n"
        );
        assert_eq!(
            fs::read_to_string(backup.join(format!("session-env/{ID}/e"))).unwrap(),
            "left over"
        );
        let again = plan_items(&src, &dst, ID, &transcript, &to, &[]).unwrap();
        assert_eq!(
            actions(&again),
            vec![(format!("projects/-w/{ID}.jsonl"), ItemAction::Same)]
        );
    }

    #[test]
    fn goes_to_the_destinations_own_copy_and_refuses_to_guess_between_two() {
        let root = tempfile::tempdir().unwrap();
        let (src, dst) = (root.path().join("a"), root.path().join("b"));
        let transcript = src.join(format!("projects/-w/{ID}.jsonl"));
        write(&transcript, "t");
        write(&dst.join(format!("projects/-elsewhere/{ID}.jsonl")), "t");
        assert_eq!(
            destination_transcript(&src, &transcript, &dst, ID).unwrap(),
            dst.join(format!("projects/-elsewhere/{ID}.jsonl"))
        );
        write(&dst.join(format!("projects/-third/{ID}.jsonl")), "t");
        assert!(matches!(
            destination_transcript(&src, &transcript, &dst, ID),
            Err(MoveError::AmbiguousDestination(paths)) if paths.len() == 2
        ));
        write(&dst.join(format!("projects/-w/{ID}.jsonl")), "t");
        assert_eq!(
            destination_transcript(&src, &transcript, &dst, ID).unwrap(),
            dst.join(format!("projects/-w/{ID}.jsonl")),
            "the one at the source's path"
        );
    }

    #[test]
    fn archives_and_restores_only_the_transcript() {
        let root = tempfile::tempdir().unwrap();
        let account = root.path().join("a");
        let transcript = account.join(format!("projects/-w/{ID}.jsonl"));
        let text = "{\"type\":\"user\"}\n".repeat(1000);
        write(&transcript, &text);
        let written = fs::metadata(&transcript).unwrap().modified().unwrap();
        write(&account.join(format!("file-history/{ID}/f")), "f");
        let archived = archive_transcript(&account, &transcript, "20260102-030405").unwrap();
        assert_eq!(
            archived,
            account.join(format!(
                "session-transfer-backups/{ID}/20260102-030405-archived/projects/-w/{ID}.jsonl.gz"
            ))
        );
        assert!(size_of(&archived) < text.len() as u64 / 10, "compressed");
        assert!(!archived.with_extension("").exists(), "no plain copy left");
        assert!(!transcript.exists());
        assert!(account.join(format!("file-history/{ID}/f")).exists());

        let listed = list_archived(&account);
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, archived);
        assert_eq!(listed[0].original, transcript);
        assert_eq!(listed[0].stamp, "20260102-030405");

        write(&transcript, "a live copy");
        assert!(restore(&account, &listed[0]).is_err());
        fs::remove_file(&transcript).unwrap();
        assert_eq!(restore(&account, &listed[0]).unwrap(), transcript);
        assert_eq!(fs::read_to_string(&transcript).unwrap(), text);
        assert_eq!(
            fs::metadata(&transcript).unwrap().modified().unwrap(),
            written,
            "restored as it was"
        );
        assert!(!account
            .join(format!(
                "session-transfer-backups/{ID}/20260102-030405-archived"
            ))
            .exists());
    }

    #[test]
    fn compresses_archives_made_before_archives_were() {
        let root = tempfile::tempdir().unwrap();
        let account = root.path().join("a");
        let old = account.join(format!(
            "session-transfer-backups/{ID}/20250102-030405-archived/projects/-w/{ID}.jsonl"
        ));
        write(&old, &"{\"type\":\"user\"}\n".repeat(1000));
        assert_eq!(list_archived(&account)[0].path, old, "listed as it is");
        let done = compress_archived(&account);
        assert_eq!(done.len(), 1);
        let (before, after) = *done[0].1.as_ref().unwrap();
        assert!(after < before / 10);
        assert!(!old.exists());
        assert_eq!(list_archived(&account)[0].path, with_suffix(&old, ".gz"));
        assert!(compress_archived(&account).is_empty(), "once");
        let info = crate::transcript::read_transcript(&with_suffix(&old, ".gz")).unwrap();
        assert!(info.first_timestamp.is_none() && !info.has_reply);
    }

    #[test]
    fn a_newer_destination_copy_is_one_that_differs_and_was_written_later() {
        let root = tempfile::tempdir().unwrap();
        let (a, b) = (root.path().join("a.jsonl"), root.path().join("b.jsonl"));
        write(&a, "x");
        write(&b, "x");
        assert!(!destination_newer(&a, &b), "identical");
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(60);
        write(&b, "y");
        fs::File::options()
            .write(true)
            .open(&b)
            .unwrap()
            .set_modified(later)
            .unwrap();
        assert!(destination_newer(&a, &b));
        assert!(!destination_newer(&b, &a));
    }

    #[test]
    fn deletes_the_source_copy_only_once_everything_arrived_intact() {
        let dir = tempfile::tempdir().unwrap();
        let (source, destination) = (dir.path().join("src"), dir.path().join("dst"));
        let transcript = source.join("projects/-code").join(format!("{ID}.jsonl"));
        write(&transcript, "{\"type\":\"user\"}\n");
        write(&source.join("file-history").join(ID).join("a"), "history");
        write(&source.join("plans/shared-plan.md"), "a plan");
        let moved = destination
            .join("projects/-code")
            .join(format!("{ID}.jsonl"));
        let items = plan_items(
            &source,
            &destination,
            ID,
            &transcript,
            &moved,
            [&"shared-plan".to_owned()],
        )
        .unwrap();
        assert_eq!(
            source_bytes(&items),
            16 + 7,
            "plans aren't the session's alone"
        );
        apply_items(&items, &dir.path().join("backups")).unwrap();

        // A copy that changed since: nothing is deleted.
        write(&moved, "{\"type\":\"user\"}\nchanged\n");
        assert!(delete_source(&items).is_err());
        assert!(transcript.is_file() && source.join("file-history").join(ID).is_dir());

        write(&moved, "{\"type\":\"user\"}\n");
        assert_eq!(delete_source(&items).unwrap(), 16 + 7);
        assert!(!transcript.exists());
        assert!(!source.join("file-history").join(ID).exists());
        assert!(
            source.join("plans/shared-plan.md").is_file(),
            "left for other sessions"
        );
        assert!(moved.is_file());
    }

    #[test]
    fn never_deletes_a_source_that_is_the_destination_by_another_name() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("src");
        let alias = dir.path().join("alias");
        let transcript = source.join("projects/-code").join(format!("{ID}.jsonl"));
        write(&transcript, "{\"type\":\"user\"}\n");
        std::os::unix::fs::symlink(&source, &alias).unwrap();
        let moved = alias.join("projects/-code").join(format!("{ID}.jsonl"));
        let items = plan_items(&source, &alias, ID, &transcript, &moved, []).unwrap();
        assert!(delete_source(&items).is_err());
        assert!(transcript.is_file());
    }
}

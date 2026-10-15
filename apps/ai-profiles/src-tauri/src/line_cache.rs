//! Line-by-line reads of append-only files, cached until the file changes.
//!
//! Claude Code transcripts reach 100 MB, a config dir holds thousands, and
//! they are read again on every window focus. A [`LineCache`] keeps what
//! reading each file found: a file that still has the length and
//! modification time it had then is not read again, and one that only grew
//! since is read on from where reading stopped.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs::File;
use std::hash::Hasher;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

/// What reading a file's whole lines in order has taken in so far.
pub trait LineScan: Clone + Default {
    /// Take in the next whole line, its `\n` included.
    fn read(&mut self, line: &[u8]);
}

/// What reading each file found, by its path: the `S` scanned from its whole
/// lines, and the `T` made of that.
pub struct LineCache<S, T> {
    reads: Mutex<HashMap<PathBuf, CachedRead<S, T>>>,
}

/// What reading a file found, kept to go on from.
#[derive(Clone)]
struct CachedRead<S, T> {
    /// The file's inode when read: a file replaced by another is read afresh.
    inode: u64,
    /// The file's length when read.
    len: u64,
    /// The file's modification time when read.
    modified: SystemTime,
    /// How far it was read: the end of its last whole line.
    offset: u64,
    /// Where that last whole line starts.
    last_line_start: u64,
    /// A hash of the file's start and of its last whole line, as read: a file
    /// that no longer has them was rewritten, not appended to. `None` when
    /// they couldn't be read.
    fingerprint: Option<u64>,
    /// What its whole lines up to `offset` say.
    scan: S,
    /// What was made of them.
    value: T,
}

impl<S, T> Default for LineCache<S, T> {
    fn default() -> Self {
        LineCache {
            reads: Mutex::default(),
        }
    }
}

impl<S: LineScan, T: Clone> LineCache<S, T> {
    /// What `finish` makes of the file at `path`, given what its whole lines
    /// say, the line after them that has no end yet (which may still be being
    /// written), and when the file was last written. From the cache if the
    /// file hasn't changed since it was read, read on from where reading
    /// stopped if it only grew, else read whole, and cached. `None` when the
    /// file can't be read. The lock isn't held while reading, so reads of
    /// other files don't wait on this one.
    pub fn read(
        &self,
        path: &Path,
        finish: impl FnOnce(&S, Option<&[u8]>, SystemTime) -> T,
    ) -> Option<T> {
        let mut file = File::open(path).ok()?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file() {
            return None;
        }
        let (inode, len, modified) = (metadata.ino(), metadata.len(), metadata.modified().ok()?);
        let cached = self.lock().get(path).cloned();
        if let Some(cached) = &cached {
            if cached.inode == inode && cached.len == len && cached.modified == modified {
                return Some(cached.value.clone());
            }
        }
        let (start, mut last_line_start, mut scan) = match cached {
            Some(cached) if grew(&mut file, &cached, inode, len) => {
                (cached.offset, cached.last_line_start, cached.scan)
            }
            _ => (0, 0, S::default()),
        };
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut reader = BufReader::new(file);
        let (read, last_line_len, tail) = read_lines(&mut reader, &mut scan);
        let offset = start + read;
        if last_line_len > 0 {
            last_line_start = offset - last_line_len;
        }
        let value = finish(&scan, tail.as_deref(), modified);
        let read = CachedRead {
            inode,
            len,
            modified,
            offset,
            last_line_start,
            fingerprint: fingerprint(reader.get_mut(), offset, last_line_start),
            scan,
            value: value.clone(),
        };
        self.lock().insert(path.to_path_buf(), read);
        Some(value)
    }

    /// Forget every file `keep` says no to.
    pub fn retain(&self, mut keep: impl FnMut(&Path) -> bool) {
        self.lock().retain(|path, _| keep(path));
    }

    /// Forget what was read of files that are gone.
    pub fn forget_gone(&self) {
        let paths: Vec<PathBuf> = self.lock().keys().cloned().collect();
        let gone: Vec<PathBuf> = paths.into_iter().filter(|path| !path.exists()).collect();
        let mut reads = self.lock();
        for path in gone {
            reads.remove(&path);
        }
    }

    /// Whether what was read of the file at `path` is cached.
    #[cfg(test)]
    pub fn contains(&self, path: &Path) -> bool {
        self.lock().contains_key(path)
    }

    /// Take the cache. Entries are whole values swapped in and out, so a
    /// panic under the lock leaves nothing half-written.
    fn lock(&self) -> MutexGuard<'_, HashMap<PathBuf, CachedRead<S, T>>> {
        self.reads.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Whether `file`, read before as `cached` and now of `inode` and `len`, only
/// grew since: it is the same file, no shorter, and still has the start and
/// the last whole line it had where reading stopped (see [`fingerprint`]).
/// Claude Code only ever appends to a transcript; a rewrite that keeps both
/// goes unseen until the file is replaced or cut shorter.
fn grew<S, T>(file: &mut File, cached: &CachedRead<S, T>, inode: u64, len: u64) -> bool {
    if cached.inode != inode || len < cached.len {
        return false;
    }
    if cached.offset == 0 {
        return true;
    }
    cached.fingerprint.is_some()
        && fingerprint(file, cached.offset, cached.last_line_start) == cached.fingerprint
}

/// How much of the start of a file [`fingerprint`] hashes.
const FINGERPRINT_HEAD: u64 = 4096;

/// A hash of `file`'s first [`FINGERPRINT_HEAD`] bytes before `offset`, and
/// of its line from `last_line_start` to `offset`. `None` when they can't be
/// read.
fn fingerprint(file: &mut File, offset: u64, last_line_start: u64) -> Option<u64> {
    let mut hasher = DefaultHasher::new();
    for (from, to) in [(0, offset.min(FINGERPRINT_HEAD)), (last_line_start, offset)] {
        let mut bytes = vec![0_u8; usize::try_from(to.checked_sub(from)?).ok()?];
        file.seek(SeekFrom::Start(from)).ok()?;
        file.read_exact(&mut bytes).ok()?;
        hasher.write(&bytes);
    }
    Some(hasher.finish())
}

/// Read the lines `reader` holds into `scan`, up to the end of its last whole
/// line. Returns how many bytes those are and how long the last of them is,
/// with the line after them that has no end yet, which Claude Code may still
/// be writing. Reading stops early, as at the end, when the file can't be
/// read on.
pub fn read_lines<S: LineScan>(
    mut reader: impl BufRead,
    scan: &mut S,
) -> (u64, u64, Option<Vec<u8>>) {
    let (mut read, mut last_line_len) = (0, 0);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return (read, last_line_len, None),
            Ok(_) if line.last() != Some(&b'\n') => return (read, last_line_len, Some(line)),
            Ok(count) => {
                read += count as u64;
                last_line_len = count as u64;
                scan.read(&line);
            }
        }
    }
}

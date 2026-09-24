//! Listing folders, for picking where a new session works. Only inside the
//! configured roots (the home folder unless set), judged after resolving
//! symlinks, so neither `..` nor a link can reach past them.

use std::fs;
use std::path::{Path, PathBuf};

use ai_profiles_core::api::{DirEntry, DirListing};

use crate::config::expand_home;
use crate::error::ApiError;

/// At most this many subfolders are returned.
const MAX_ENTRIES: usize = 500;

/// The canonical form of each root that exists.
pub fn canonical_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect()
}

/// `requested` resolved, or an error if it isn't a folder inside `roots`.
pub fn resolve_inside(
    roots: &[PathBuf],
    home: &Path,
    requested: &str,
) -> Result<PathBuf, ApiError> {
    let path = expand_home(requested, home);
    if !path.is_absolute() {
        return Err(ApiError::invalid("The folder must be an absolute path."));
    }
    let resolved = path
        .canonicalize()
        .map_err(|_| ApiError::not_found(format!("{requested} doesn't exist.")))?;
    if !canonical_roots(roots)
        .iter()
        .any(|root| resolved.starts_with(root))
    {
        return Err(ApiError::forbidden(
            "forbidden_path",
            format!("{requested} is outside the folders this server lets clients use."),
        ));
    }
    if !resolved.is_dir() {
        return Err(ApiError::invalid(format!("{requested} isn't a folder.")));
    }
    Ok(resolved)
}

pub fn list(
    roots: &[PathBuf],
    home: &Path,
    requested: Option<&str>,
    show_hidden: bool,
) -> Result<DirListing, ApiError> {
    let home_text = home.display().to_string();
    let path = resolve_inside(roots, home, requested.unwrap_or(&home_text))?;
    let mut entries: Vec<DirEntry> = fs::read_dir(&path)?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let name = entry.file_name().to_str()?.to_owned();
            (show_hidden || !name.starts_with('.')).then(|| DirEntry {
                path: path.join(&name).display().to_string(),
                name,
            })
        })
        .collect();
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    let truncated = entries.len() > MAX_ENTRIES;
    entries.truncate(MAX_ENTRIES);
    let canonical = canonical_roots(roots);
    let parent = path
        .parent()
        .filter(|parent| canonical.iter().any(|root| parent.starts_with(root)))
        .map(|parent| parent.display().to_string());
    Ok(DirListing {
        path: path.display().to_string(),
        parent,
        home: home_text,
        entries,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().canonicalize().unwrap().join("home");
        for folder in ["code/api", "code/web", ".hidden", "Docs"] {
            fs::create_dir_all(home.join(folder)).unwrap();
        }
        fs::write(home.join("file.txt"), "").unwrap();
        fs::create_dir_all(dir.path().join("outside")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("outside"), home.join("escape")).unwrap();
        (dir, home)
    }

    #[test]
    fn lists_subfolders_of_home_by_default() {
        let (_dir, home) = tree();
        let listing = list(std::slice::from_ref(&home), &home, None, false).unwrap();
        let names: Vec<&str> = listing.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["code", "Docs", "escape"]);
        assert_eq!(listing.parent, None, "home is the root");
        assert!(!listing.truncated);

        let hidden = list(std::slice::from_ref(&home), &home, None, true).unwrap();
        assert!(hidden.entries.iter().any(|e| e.name == ".hidden"));
    }

    #[test]
    fn goes_down_with_a_parent_link_and_accepts_tilde() {
        let (_dir, home) = tree();
        let listing = list(std::slice::from_ref(&home), &home, Some("~/code"), false).unwrap();
        assert_eq!(listing.entries.len(), 2);
        assert_eq!(listing.parent.as_deref(), Some(home.to_str().unwrap()));
    }

    #[test]
    fn refuses_to_leave_the_roots_by_dots_or_links() {
        let (_dir, home) = tree();
        let roots = [home.clone()];
        let dots = list(
            &roots,
            &home,
            Some(&format!("{}/../", home.display())),
            false,
        );
        assert_eq!(dots.unwrap_err().code, "forbidden_path");
        let link = list(&roots, &home, Some("~/escape"), false);
        assert_eq!(link.unwrap_err().code, "forbidden_path");
        assert_eq!(
            list(&roots, &home, Some("relative"), false)
                .unwrap_err()
                .code,
            "invalid_request"
        );
        assert_eq!(
            list(&roots, &home, Some("~/nope"), false).unwrap_err().code,
            "not_found"
        );
        assert_eq!(
            list(&roots, &home, Some("~/file.txt"), false)
                .unwrap_err()
                .code,
            "invalid_request"
        );
    }
}

//! The wrapper's name in the menu bar.
//!
//! macOS names the running app in the menu bar (the bold item beside the Apple
//! menu) after its localized `CFBundleName`, and the Dock after its localized
//! `CFBundleDisplayName`. The wrapper's `Info.plist` must keep the vendor's
//! `CFBundleName`, since Electron finds its helper apps by it (see
//! [`super::info_plist::patch`]), so the profile's name goes into each
//! language's `InfoPlist.strings` instead: macOS reads the localized value for
//! the menu bar, while Electron's helper lookup still reads the one in
//! `Info.plist`.

use std::fs;
use std::path::Path;
use std::process::Command;

use plist::{Dictionary, Value};

use crate::error::AppResult;

const STRINGS_FILE: &str = "InfoPlist.strings";

/// Give every localization in `resources` the name `name`, keeping whatever
/// else the vendor's `InfoPlist.strings` there says (ChatGPT's carry its
/// permission prompts). A bundle with no localizations gets an English one.
///
/// A vendor file that can't be read is left as it is: that language shows the
/// vendor's name, which is what it showed before.
pub fn write(resources: &Path, name: &str) -> AppResult<()> {
    let mut localizations: Vec<_> = fs::read_dir(resources)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.extension().is_some_and(|ext| ext == "lproj"))
        .collect();
    if localizations.is_empty() {
        let english = resources.join("en.lproj");
        fs::create_dir_all(&english)?;
        localizations.push(english);
    }
    for localization in localizations {
        let path = localization.join(STRINGS_FILE);
        let Some(mut strings) = read_strings(&path) else {
            continue;
        };
        strings.insert("CFBundleName".into(), Value::String(name.to_owned()));
        strings.insert("CFBundleDisplayName".into(), Value::String(name.to_owned()));
        // A new file renamed into place, never an edit of the one cloned from
        // the vendor's bundle.
        let staged = localization.join(format!(".{STRINGS_FILE}.tmp"));
        Value::Dictionary(strings)
            .to_file_xml(&staged)
            .map_err(|err| std::io::Error::other(err.to_string()))?;
        fs::rename(&staged, &path)?;
    }
    Ok(())
}

/// The strings in the `InfoPlist.strings` at `path`: empty when there is none,
/// `None` when there is one that can't be read.
///
/// These files come in the old text format as often as in XML or binary, which
/// the `plist` crate can't read, so `plutil` turns them into XML first.
fn read_strings(path: &Path) -> Option<Dictionary> {
    if !path.exists() {
        return Some(Dictionary::new());
    }
    let output = Command::new("/usr/bin/plutil")
        .args(["-convert", "xml1", "-o", "-"])
        .arg(path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Value::from_reader_xml(output.stdout.as_slice())
        .ok()?
        .into_dictionary()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings_at(path: &Path) -> Dictionary {
        read_strings(path).expect("readable strings")
    }

    fn text<'a>(strings: &'a Dictionary, key: &str) -> Option<&'a str> {
        strings.get(key).and_then(Value::as_string)
    }

    #[test]
    fn names_every_localization_and_keeps_the_vendors_other_strings() {
        let dir = tempfile::tempdir().unwrap();
        let resources = dir.path();
        fs::create_dir_all(resources.join("en.lproj")).unwrap();
        fs::create_dir_all(resources.join("sv.lproj")).unwrap();
        // The old text format, as ChatGPT ships it.
        fs::write(
            resources.join("sv.lproj").join(STRINGS_FILE),
            "\"NSCameraUsageDescription\" = \"Kameran behövs\";\n\"CFBundleName\" = \"Vendor\";\n",
        )
        .unwrap();

        write(resources, "Claude (Work)").unwrap();

        let english = strings_at(&resources.join("en.lproj").join(STRINGS_FILE));
        assert_eq!(text(&english, "CFBundleName"), Some("Claude (Work)"));
        assert_eq!(text(&english, "CFBundleDisplayName"), Some("Claude (Work)"));

        let swedish = strings_at(&resources.join("sv.lproj").join(STRINGS_FILE));
        assert_eq!(text(&swedish, "CFBundleName"), Some("Claude (Work)"));
        assert_eq!(
            text(&swedish, "NSCameraUsageDescription"),
            Some("Kameran behövs")
        );
        assert!(!resources.join("sv.lproj/.InfoPlist.strings.tmp").exists());
    }

    #[test]
    fn a_bundle_without_localizations_gets_an_english_one() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "Claude (Work)").unwrap();
        let english = strings_at(&dir.path().join("en.lproj").join(STRINGS_FILE));
        assert_eq!(text(&english, "CFBundleName"), Some("Claude (Work)"));
    }

    #[test]
    fn leaves_an_unreadable_file_as_it_is() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("en.lproj")).unwrap();
        let path = dir.path().join("en.lproj").join(STRINGS_FILE);
        fs::write(&path, "this is { not a strings file").unwrap();
        write(dir.path(), "Claude (Work)").unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "this is { not a strings file"
        );
    }

    #[test]
    fn replacing_a_file_never_writes_through_to_the_one_it_was_cloned_from() {
        let dir = tempfile::tempdir().unwrap();
        let lproj = dir.path().join("en.lproj");
        fs::create_dir_all(&lproj).unwrap();
        let original = dir.path().join("vendor.strings");
        fs::write(&original, "\"Key\" = \"Value\";\n").unwrap();
        // A hard link stands in for a file shared with the vendor bundle.
        fs::hard_link(&original, lproj.join(STRINGS_FILE)).unwrap();

        write(dir.path(), "Claude (Work)").unwrap();

        assert_eq!(
            fs::read_to_string(&original).unwrap(),
            "\"Key\" = \"Value\";\n"
        );
    }
}

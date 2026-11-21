//! Rules for names inside an archive: where an entry may land, which links are acceptable and which
//! file types are blocked. Every name here comes from the poster.

use std::fmt;
use std::path::{Path, PathBuf};

use fangst_core::path::safe_file_name;

/// Why an archive entry is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsafe {
    /// Starts at a root or a drive instead of the job folder.
    Absolute,
    /// Has a `..` part, the classic way out of the extraction folder.
    ParentDirectory,
    /// A part that isn't a plain file name on every platform, such as `CON`, `a:b` or `name.`.
    InvalidName,
    /// A link whose target lies outside the job folder.
    LinkOutside,
    /// A link whose target only becomes known while decoding, too late to check.
    LinkUnchecked,
}

impl fmt::Display for Unsafe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Absolute => "is an absolute path",
            Self::ParentDirectory => "climbs out of the job folder with `..`",
            Self::InvalidName => "is not a valid file name",
            Self::LinkOutside => "is a link pointing outside the job folder",
            Self::LinkUnchecked => "is a link whose target can't be checked before extraction",
        })
    }
}

/// Where a link points, as the archive records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkTarget {
    /// Relative to the folder holding the link, as a symlink is.
    FromLink(String),
    /// A member name relative to the archive root, as a hard link or file copy is.
    FromRoot(String),
    Unknown,
}

fn starts_with_drive(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

/// The relative path an entry named `name` extracts to, if it stays inside the folder.
///
/// Both `/` and `\` separate parts, since archives made on Windows use either. Every part must
/// already be what [`safe_file_name`] would make of it; anything it would change is refused
/// instead of renamed, so two entries can't be bent onto the same file.
pub fn entry_path(name: &str) -> Result<PathBuf, Unsafe> {
    if name.starts_with(['/', '\\']) || starts_with_drive(name) {
        return Err(Unsafe::Absolute);
    }
    let mut path = PathBuf::new();
    for part in name.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => return Err(Unsafe::ParentDirectory),
            _ if safe_file_name(part).as_deref() != Some(part) => {
                return Err(Unsafe::InvalidName);
            }
            _ => path.push(part),
        }
    }
    if path.as_os_str().is_empty() {
        return Err(Unsafe::InvalidName);
    }
    Ok(path)
}

/// Checks a link entry at the relative path `entry` against where it points.
pub fn check_link(entry: &Path, target: &LinkTarget) -> Result<(), Unsafe> {
    let target = match target {
        LinkTarget::Unknown => return Err(Unsafe::LinkUnchecked),
        LinkTarget::FromRoot(name) => {
            return entry_path(name).map(drop).map_err(|_| Unsafe::LinkOutside);
        }
        LinkTarget::FromLink(target) => target,
    };
    if target.starts_with(['/', '\\']) || starts_with_drive(target) {
        return Err(Unsafe::LinkOutside);
    }
    let mut depth = entry.components().count().saturating_sub(1);
    for part in target.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => depth = depth.checked_sub(1).ok_or(Unsafe::LinkOutside)?,
            _ => depth += 1,
        }
    }
    Ok(())
}

/// Whether the last part of `name` ends in one of the `blocked` extensions, ignoring case. The
/// list may give extensions with or without the leading dot.
#[must_use]
pub fn is_blocked(name: &str, blocked: &[String]) -> bool {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let Some((_, extension)) = last.rsplit_once('.') else {
        return false;
    };
    blocked
        .iter()
        .any(|b| b.trim_start_matches('.').eq_ignore_ascii_case(extension))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names_and_subfolders_are_kept() {
        assert_eq!(entry_path("show.mkv"), Ok(PathBuf::from("show.mkv")));
        assert_eq!(
            entry_path("Sample/show-sample.mkv"),
            Ok(Path::new("Sample").join("show-sample.mkv"))
        );
        assert_eq!(
            entry_path("Subs\\en.srt"),
            Ok(Path::new("Subs").join("en.srt"))
        );
        assert_eq!(entry_path("./a//b.txt"), Ok(Path::new("a").join("b.txt")));
    }

    #[test]
    fn parent_parts_are_refused_even_when_they_would_stay_inside() {
        for name in [
            "../evil.txt",
            "a/../../evil.txt",
            "a\\..\\b",
            "a/../b",
            "..",
        ] {
            assert_eq!(entry_path(name), Err(Unsafe::ParentDirectory), "{name}");
        }
    }

    #[test]
    fn absolute_paths_and_drives_are_refused() {
        for name in [
            "/etc/passwd",
            "\\Windows\\evil.dll",
            "\\\\server\\share\\x",
            "C:\\Windows\\evil.txt",
            "c:evil.txt",
            "C:/x",
        ] {
            assert_eq!(entry_path(name), Err(Unsafe::Absolute), "{name}");
        }
    }

    #[test]
    fn names_safe_file_name_would_change_are_refused() {
        for name in [
            "CON",
            "nul.txt",
            "dir/aux.mkv",
            "a:b.mkv",
            "show.mkv:hidden",
            "trailing.",
            "tab\there",
            "",
            "/",
            "./",
        ] {
            assert!(entry_path(name).is_err(), "{name:?}");
        }
        assert_eq!(entry_path("CON"), Err(Unsafe::InvalidName));
    }

    #[test]
    fn symlinks_may_point_within_the_folder_only() {
        let at = |p: &str| PathBuf::from(p);
        let link = |t: &str| LinkTarget::FromLink(t.to_owned());
        assert_eq!(check_link(&at("a/link"), &link("../show.mkv")), Ok(()));
        assert_eq!(check_link(&at("link"), &link("Sample/x.mkv")), Ok(()));
        assert_eq!(
            check_link(&at("link"), &link("../outside")),
            Err(Unsafe::LinkOutside)
        );
        assert_eq!(
            check_link(&at("a/link"), &link("../../outside")),
            Err(Unsafe::LinkOutside)
        );
        assert_eq!(
            check_link(&at("link"), &link("/etc/passwd")),
            Err(Unsafe::LinkOutside)
        );
        assert_eq!(
            check_link(&at("link"), &link("C:\\Windows")),
            Err(Unsafe::LinkOutside)
        );
        assert_eq!(
            check_link(&at("link"), &LinkTarget::Unknown),
            Err(Unsafe::LinkUnchecked)
        );
    }

    #[test]
    fn hard_links_name_a_member_from_the_root() {
        let hard = |t: &str| LinkTarget::FromRoot(t.to_owned());
        assert_eq!(check_link(Path::new("a/b"), &hard("show.mkv")), Ok(()));
        assert_eq!(
            check_link(Path::new("a/b"), &hard("../x")),
            Err(Unsafe::LinkOutside)
        );
    }

    #[test]
    fn blocked_extensions_ignore_case_and_dots() {
        let blocked = vec!["exe".to_owned(), ".lnk".to_owned()];
        assert!(is_blocked("setup.exe", &blocked));
        assert!(is_blocked("Sub/SETUP.EXE", &blocked));
        assert!(is_blocked("Watch Now.lnk", &blocked));
        assert!(!is_blocked("show.mkv", &blocked));
        assert!(!is_blocked("exe", &blocked));
        assert!(!is_blocked("show.exe.mkv", &blocked));
    }
}

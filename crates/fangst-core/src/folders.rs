//! Checks that a folder can be written and that two folders can share files through hardlinks.
//!
//! Media managers move finished downloads into the library by hardlink when both folders are on
//! the same filesystem, which takes no time and no extra space. Across filesystems (or Docker
//! volumes mounted separately) the link fails and every file is copied instead.

use std::io::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderCheck {
    /// The folder as checked, made absolute.
    pub path: PathBuf,
    /// The folder did not exist and was created.
    pub created: bool,
    pub writable: bool,
    /// Why it isn't usable, when it isn't.
    pub problem: Option<String>,
}

/// Creates `path` if needed and writes and removes a small file in it.
#[must_use]
pub fn check_folder(path: &Path) -> FolderCheck {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let existed = path.is_dir();
    let mut check = FolderCheck {
        path: path.clone(),
        created: false,
        writable: false,
        problem: None,
    };
    if path.exists() && !existed {
        check.problem = Some("it is a file, not a folder".to_owned());
        return check;
    }
    if let Err(e) = std::fs::create_dir_all(&path) {
        check.problem = Some(format!("can't create it: {e}"));
        return check;
    }
    check.created = !existed;
    let probe = path.join(probe_name());
    let written = std::fs::File::create(&probe).and_then(|mut f| f.write_all(b"fangst"));
    let _ = std::fs::remove_file(&probe);
    match written {
        Ok(()) => check.writable = true,
        Err(e) => check.problem = Some(format!("can't write to it: {e}")),
    }
    check
}

/// Whether a file in `from` can be hardlinked into `to`. Both folders must exist and be writable.
///
/// # Errors
/// Why the link failed, such as the folders being on different filesystems.
pub fn check_hardlink(from: &Path, to: &Path) -> Result<(), String> {
    let name = probe_name();
    let source = from.join(&name);
    let target = to.join(&name);
    std::fs::write(&source, b"fangst").map_err(|e| format!("can't write a test file: {e}"))?;
    let linked = std::fs::hard_link(&source, &target);
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_file(&source);
    linked.map_err(|e| {
        format!("{e}; the folders are probably on different disks or separately mounted volumes")
    })
}

fn probe_name() -> String {
    let suffix = crate::secret::random_token()
        .map(|t| t.chars().take(12).collect::<String>())
        .unwrap_or_default();
    format!(".fangst-check-{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fangst-folders-{}",
            crate::secret::random_token().unwrap()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_missing_folder_is_created_and_left_empty() {
        let dir = temp();
        let check = check_folder(&dir.join("a").join("b"));
        assert!(check.created && check.writable, "{check:?}");
        assert!(check.path.is_absolute());
        assert_eq!(std::fs::read_dir(&check.path).unwrap().count(), 0);
        let again = check_folder(&dir.join("a").join("b"));
        assert!(!again.created && again.writable);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_is_not_a_folder() {
        let dir = temp();
        std::fs::write(dir.join("f"), b"x").unwrap();
        let check = check_folder(&dir.join("f"));
        assert!(!check.writable);
        assert!(check.problem.unwrap().contains("file"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folders_on_one_disk_can_share_files_and_leave_nothing_behind() {
        let dir = temp();
        let (a, b) = (dir.join("downloads"), dir.join("media"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        assert_eq!(check_hardlink(&a, &b), Ok(()));
        assert_eq!(std::fs::read_dir(&a).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&b).unwrap().count(), 0);
        assert!(check_hardlink(&a, &dir.join("missing")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

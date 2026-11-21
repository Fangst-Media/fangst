//! Finding the archives in a job folder and which files belong to each.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// Suffix of a file being written; it only takes its real name once its checksum matched.
pub const PART_SUFFIX: &str = ".fangst-part";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Format {
    Rar,
    SevenZip,
}

/// One archive, split over one or more volumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveSet {
    pub format: Format,
    /// In volume order; the first one is opened.
    pub volumes: Vec<PathBuf>,
}

impl ArchiveSet {
    /// The first volume's file name, for messages.
    #[must_use]
    pub fn name(&self) -> String {
        self.volumes
            .first()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// The folder the archive's entries extract into.
    #[must_use]
    pub fn folder(&self) -> &Path {
        self.volumes
            .first()
            .and_then(|p| p.parent())
            .unwrap_or_else(|| Path::new(""))
    }
}

/// What a walk through the job folder found.
#[derive(Debug, Default)]
pub struct Walk {
    pub files: Vec<PathBuf>,
    /// Symlinks and junctions. Downloads never create them, so one here was planted.
    pub links: Vec<PathBuf>,
}

/// Every file below `folder`, without following links. Unfinished extraction output is skipped.
pub fn walk(folder: &Path) -> io::Result<Walk> {
    let mut found = Walk::default();
    let mut pending = vec![folder.to_owned()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                found.links.push(path);
            } else if kind.is_dir() {
                pending.push(path);
            } else if !path.to_string_lossy().ends_with(PART_SUFFIX) {
                found.files.push(path);
            }
        }
    }
    found.files.sort();
    found.links.sort();
    Ok(found)
}

/// Which volume of which set a file name is: `(format, base name, position)`.
fn classify(name: &str) -> Option<(Format, String, u32)> {
    let lower = name.to_ascii_lowercase();
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if let Some(stem) = lower.strip_suffix(".rar") {
        if let Some((base, part)) = stem.rsplit_once(".part")
            && digits(part)
        {
            return Some((Format::Rar, base.to_owned(), part.parse().ok()?));
        }
        return Some((Format::Rar, stem.to_owned(), 0));
    }
    if let Some((stem, number)) = lower.rsplit_once('.') {
        // Old-style volumes: .rar first, then .r00, .r01, ...
        if let Some(n) = number.strip_prefix('r')
            && (2..=3).contains(&n.len())
            && digits(n)
        {
            return Some((Format::Rar, stem.to_owned(), n.parse::<u32>().ok()? + 1));
        }
        if number == "7z" {
            return Some((Format::SevenZip, stem.to_owned(), 0));
        }
        // 7-Zip splits a finished archive into .7z.001, .7z.002, ...
        if let Some(base) = stem.strip_suffix(".7z")
            && number.len() == 3
            && digits(number)
        {
            return Some((Format::SevenZip, base.to_owned(), number.parse().ok()?));
        }
    }
    None
}

/// Groups the archive volumes among `files` into sets, by folder and base name, ignoring case.
#[must_use]
pub fn find(files: &[PathBuf]) -> Vec<ArchiveSet> {
    type Key = (PathBuf, Format, String);
    let mut sets: BTreeMap<Key, Vec<(u32, PathBuf)>> = BTreeMap::new();
    for path in files {
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some((format, base, position)) = classify(name) else {
            continue;
        };
        let folder = path.parent().map(Path::to_owned).unwrap_or_default();
        sets.entry((folder, format, base))
            .or_default()
            .push((position, path.clone()));
    }
    sets.into_iter()
        .map(|((_, format, _), mut volumes)| {
            volumes.sort();
            ArchiveSet {
                format,
                volumes: volumes.into_iter().map(|(_, p)| p).collect(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(sets: &[ArchiveSet]) -> Vec<(Format, Vec<String>)> {
        sets.iter()
            .map(|s| {
                (
                    s.format,
                    s.volumes
                        .iter()
                        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
                        .collect(),
                )
            })
            .collect()
    }

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(|n| Path::new("job").join(n)).collect()
    }

    #[test]
    fn new_style_rar_volumes_are_ordered_by_part_number() {
        let files = paths(&[
            "Show.part10.rar",
            "Show.part02.rar",
            "Show.part01.rar",
            "show.nfo",
        ]);
        assert_eq!(
            names(&find(&files)),
            [(
                Format::Rar,
                vec![
                    "Show.part01.rar".to_owned(),
                    "Show.part02.rar".to_owned(),
                    "Show.part10.rar".to_owned()
                ]
            )]
        );
    }

    #[test]
    fn old_style_rar_volumes_start_with_the_rar_file() {
        let files = paths(&["show.r01", "show.r00", "SHOW.rar", "show.r100"]);
        assert_eq!(
            names(&find(&files)),
            [(
                Format::Rar,
                vec![
                    "SHOW.rar".to_owned(),
                    "show.r00".to_owned(),
                    "show.r01".to_owned(),
                    "show.r100".to_owned()
                ]
            )]
        );
    }

    #[test]
    fn seven_zip_archives_and_split_volumes() {
        let files = paths(&["b.7z.002", "b.7z.001", "a.7z", "c.zip", "d.r", "e.rx1"]);
        assert_eq!(
            names(&find(&files)),
            [
                (Format::SevenZip, vec!["a.7z".to_owned()]),
                (
                    Format::SevenZip,
                    vec!["b.7z.001".to_owned(), "b.7z.002".to_owned()]
                ),
            ]
        );
    }

    #[test]
    fn archives_in_different_folders_stay_apart() {
        let files = vec![
            Path::new("job").join("x.rar"),
            Path::new("job").join("sub").join("x.rar"),
        ];
        assert_eq!(find(&files).len(), 2);
    }
}

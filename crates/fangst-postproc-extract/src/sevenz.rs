//! 7z archives through `sevenz-rust2`, including AES encryption and archives split into `.7z.001`,
//! `.7z.002`, ... volumes.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use sevenz_rust2::{Archive, ArchiveEntry, ArchiveReader, EntryDecision, Password};

use crate::names::LinkTarget;
use crate::sets::ArchiveSet;
use crate::{Entry, EntryKind, Error, Output};

const WINDOWS_REPARSE_POINT: u32 = 0x400;
/// 7-Zip sets this when the high 16 bits of the attributes hold a Unix mode.
const UNIX_EXTENSION: u32 = 0x8000;
const UNIX_TYPE_MASK: u32 = 0o170_000;
const UNIX_SYMLINK: u32 = 0o120_000;

/// The volumes of a split archive read as one stream; 7-Zip splits at arbitrary byte offsets.
struct Volumes {
    files: Vec<File>,
    /// Offset of each volume's first byte in the joined stream, plus the total length at the end.
    starts: Vec<u64>,
    position: u64,
}

impl Volumes {
    fn open(paths: &[PathBuf]) -> io::Result<Self> {
        let mut files = Vec::with_capacity(paths.len());
        let mut starts = vec![0];
        let mut total = 0u64;
        for path in paths {
            let file = File::open(path)?;
            total = total.saturating_add(file.metadata()?.len());
            files.push(file);
            starts.push(total);
        }
        Ok(Self {
            files,
            starts,
            position: 0,
        })
    }

    fn len(&self) -> u64 {
        self.starts.last().copied().unwrap_or(0)
    }
}

impl Read for Volumes {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // The last start is the total length, so a position at or past the end finds no volume.
        let Some(index) = self.starts.windows(2).position(|w| self.position < w[1]) else {
            return Ok(0);
        };
        let offset = self.position - self.starts[index];
        let left = self.starts[index + 1] - self.position;
        let want = buf.len().min(usize::try_from(left).unwrap_or(usize::MAX));
        let file = &mut self.files[index];
        file.seek(SeekFrom::Start(offset))?;
        let read = file.read(&mut buf[..want])?;
        self.position += read as u64;
        Ok(read)
    }
}

impl Seek for Volumes {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(n) => Some(n),
            SeekFrom::End(n) => self.len().checked_add_signed(n),
            SeekFrom::Current(n) => self.position.checked_add_signed(n),
        };
        self.position = target
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before the start"))?;
        Ok(self.position)
    }
}

fn password_of(password: Option<&str>) -> Password {
    password.map_or_else(Password::empty, Password::from)
}

fn error(set: &ArchiveSet, password: Option<&str>, e: sevenz_rust2::Error) -> Error {
    let archive = set.name();
    match e {
        sevenz_rust2::Error::PasswordRequired => Error::PasswordMissing { archive },
        sevenz_rust2::Error::MaybeBadPassword(_) => Error::PasswordWrong { archive },
        // Headers decrypted with the wrong key are noise, which shows up as a broken header.
        sevenz_rust2::Error::NextHeaderCrcMismatch
        | sevenz_rust2::Error::BadTerminatedHeader(_)
        | sevenz_rust2::Error::ChecksumVerificationFailed
            if password.is_some() =>
        {
            Error::PasswordWrong { archive }
        }
        other => Error::Damaged {
            archive,
            message: other.to_string(),
        },
    }
}

fn kind(entry: &ArchiveEntry) -> EntryKind {
    if entry.is_directory {
        return EntryKind::Directory;
    }
    let attributes = entry.windows_attributes;
    let unix_link =
        attributes & UNIX_EXTENSION != 0 && (attributes >> 16) & UNIX_TYPE_MASK == UNIX_SYMLINK;
    if entry.has_windows_attributes && (unix_link || attributes & WINDOWS_REPARSE_POINT != 0) {
        // 7-Zip stores the target as the entry's data, readable only by decoding it.
        return EntryKind::Link(LinkTarget::Unknown);
    }
    EntryKind::File
}

fn entry(archive_entry: &ArchiveEntry) -> Entry {
    Entry {
        name: archive_entry.name.clone(),
        kind: kind(archive_entry),
    }
}

fn open(set: &ArchiveSet, password: Option<&str>) -> Result<(Archive, Volumes), Error> {
    let mut source = Volumes::open(&set.volumes).map_err(|e| Error::io(&set.volumes[0], &e))?;
    let archive =
        Archive::read(&mut source, &password_of(password)).map_err(|e| error(set, password, e))?;
    let encrypted = (0..archive.files.len()).any(|i| archive.is_file_encrypted(i));
    if encrypted && password.is_none() {
        return Err(Error::PasswordMissing {
            archive: set.name(),
        });
    }
    Ok((archive, source))
}

pub fn list(set: &ArchiveSet, password: Option<&str>) -> Result<Vec<Entry>, Error> {
    let (archive, _) = open(set, password)?;
    Ok(archive
        .files
        .iter()
        .filter(|e| !e.is_anti_item)
        .map(entry)
        .collect())
}

/// Copies `data` into `out`, failing when the bytes don't match the CRC the archive recorded.
fn copy_checked(data: &mut dyn Read, out: &mut dyn Write, crc: Option<u32>) -> io::Result<()> {
    let mut hasher = crc32fast::Hasher::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = data.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        out.write_all(&buffer[..read])?;
    }
    match crc {
        Some(expected) if hasher.finalize() != expected => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "the data does not match its checksum",
        )),
        _ => Ok(()),
    }
}

/// Writes each entry `target` returns a path for; the others are decoded and checked, then dropped.
pub fn extract(
    set: &ArchiveSet,
    password: Option<&str>,
    target: &dyn Fn(&Entry) -> Option<PathBuf>,
    out: &mut Output,
) -> Result<(), Error> {
    let (archive, source) = open(set, password)?;
    let encrypted = (0..archive.files.len()).any(|i| archive.is_file_encrypted(i));
    let mut reader = ArchiveReader::from_archive(archive, source, password_of(password));
    let mut failure = None;
    let result = reader.for_each_entries_with_decision(|archive_entry, data| {
        if archive_entry.is_anti_item {
            return Ok(EntryDecision::Continue);
        }
        let item = entry(archive_entry);
        let Some(path) = target(&item) else {
            return Ok(EntryDecision::Continue);
        };
        let crc = archive_entry
            .has_crc
            .then(|| u32::try_from(archive_entry.crc).ok())
            .flatten();
        let written = out.file(&path, |file: &mut dyn Write| {
            copy_checked(data, file, crc).map_err(|e| {
                if encrypted {
                    // A wrong key decodes to noise that fails the codec or the checksum.
                    Error::PasswordWrong {
                        archive: set.name(),
                    }
                } else {
                    Error::Damaged {
                        archive: set.name(),
                        message: format!("{}: {e}", item.name),
                    }
                }
            })
        });
        match written {
            Ok(()) => Ok(EntryDecision::Continue),
            Err(e) => {
                failure = Some(e);
                Ok(EntryDecision::Stop)
            }
        }
    });
    if let Some(failure) = failure {
        return Err(failure);
    }
    result.map_err(|e| error(set, password, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes_read_and_seek_across_file_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let parts: Vec<PathBuf> = [b"abc".as_slice(), b"", b"defg"]
            .iter()
            .enumerate()
            .map(|(i, bytes)| {
                let path = dir.path().join(format!("x.7z.{:03}", i + 1));
                std::fs::write(&path, bytes).unwrap();
                path
            })
            .collect();
        let mut volumes = Volumes::open(&parts).unwrap();
        let mut all = Vec::new();
        volumes.read_to_end(&mut all).unwrap();
        assert_eq!(all, b"abcdefg");
        assert_eq!(volumes.seek(SeekFrom::End(-5)).unwrap(), 2);
        let mut two = [0u8; 2];
        volumes.read_exact(&mut two).unwrap();
        assert_eq!(&two, b"cd");
        assert!(volumes.seek(SeekFrom::Current(-10)).is_err());
    }

    #[test]
    fn unix_symlinks_and_reparse_points_are_links() {
        let mut link = ArchiveEntry::new_file("l");
        link.has_windows_attributes = true;
        link.windows_attributes = UNIX_EXTENSION | (0o120_777 << 16);
        assert_eq!(kind(&link), EntryKind::Link(LinkTarget::Unknown));
        link.windows_attributes = WINDOWS_REPARSE_POINT;
        assert_eq!(kind(&link), EntryKind::Link(LinkTarget::Unknown));
        let mut file = ArchiveEntry::new_file("f");
        file.has_windows_attributes = true;
        file.windows_attributes = UNIX_EXTENSION | (0o100_644 << 16);
        assert_eq!(kind(&file), EntryKind::File);
        assert_eq!(
            kind(&ArchiveEntry::new_directory("d")),
            EntryKind::Directory
        );
    }
}

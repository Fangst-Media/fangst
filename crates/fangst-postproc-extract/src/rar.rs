//! RAR archives through `rar-rs`, a pure-Rust reader for RAR 1.5 to 7, including volume sets and
//! both kinds of encryption.

use std::io::Write;
use std::path::PathBuf;

use rar_rs::{ArchiveReader, OpenOptions, RarError};

use crate::names::LinkTarget;
use crate::sets::ArchiveSet;
use crate::{Entry, EntryKind, Error, Output};

/// Unix file type bits as RAR4 stores them for a symlink archived on Unix.
const UNIX_TYPE_MASK: u64 = 0o170_000;
const UNIX_SYMLINK: u64 = 0o120_000;
const WINDOWS_REPARSE_POINT: u64 = 0x400;
/// RAR5 redirect types that name another member instead of a filesystem path.
const HARD_LINK: u64 = 4;
const FILE_COPY: u64 = 5;

fn open(set: &ArchiveSet, password: Option<&str>) -> Result<ArchiveReader, Error> {
    let mut options = OpenOptions::new();
    if let Some(password) = password {
        options = options.password(password);
    }
    // rar-rs finds the other volumes of the set beside the one it opens.
    ArchiveReader::open_with(&set.volumes[0], options).map_err(|e| error(set, None, e))
}

fn error(set: &ArchiveSet, member: Option<&str>, e: RarError) -> Error {
    let archive = set.name();
    match e {
        RarError::Encrypted(_) => Error::PasswordMissing { archive },
        RarError::WrongPassword => Error::PasswordWrong { archive },
        other => Error::Damaged {
            archive,
            message: match member {
                Some(member) => format!("{member}: {other}"),
                None => other.to_string(),
            },
        },
    }
}

fn kind(entry: &rar_rs::ArchiveEntry) -> EntryKind {
    if let Some((kind, target)) = entry.redirect() {
        return EntryKind::Link(if matches!(kind, HARD_LINK | FILE_COPY) {
            LinkTarget::FromRoot(target)
        } else {
            LinkTarget::FromLink(target)
        });
    }
    if entry.is_dir() {
        return EntryKind::Directory;
    }
    let attributes = entry.attributes();
    let unix_link = entry.host_os() == 1 && attributes & UNIX_TYPE_MASK == UNIX_SYMLINK;
    let windows_link = entry.host_os() == 0 && attributes & WINDOWS_REPARSE_POINT != 0;
    if unix_link || windows_link {
        // RAR4 keeps the target in the member's data, readable only by decoding it.
        return EntryKind::Link(LinkTarget::Unknown);
    }
    EntryKind::File
}

pub fn list(set: &ArchiveSet, password: Option<&str>) -> Result<Vec<Entry>, Error> {
    let reader = open(set, password)?;
    Ok(reader
        .entries()
        .map(|e| {
            let entry = e.metadata();
            Entry {
                name: entry.name().to_owned(),
                kind: kind(entry),
            }
        })
        .collect())
}

/// Writes each entry `target` returns a path for; the others are skipped. rar-rs checks every
/// member's CRC32 or BLAKE2 before `copy_entry_to` returns, so a damaged member never gets its name.
pub fn extract(
    set: &ArchiveSet,
    password: Option<&str>,
    target: &dyn Fn(&Entry) -> Option<PathBuf>,
    out: &mut Output,
) -> Result<(), Error> {
    let mut reader = open(set, password)?;
    let members: Vec<(rar_rs::EntryId, Entry)> = reader
        .entries()
        .map(|e| {
            let entry = e.metadata();
            (
                e.id(),
                Entry {
                    name: entry.name().to_owned(),
                    kind: kind(entry),
                },
            )
        })
        .collect();
    for (id, entry) in members {
        let Some(path) = target(&entry) else {
            continue;
        };
        out.file(&path, |file: &mut dyn Write| {
            reader
                .copy_entry_to(id, file)
                .map(drop)
                .map_err(|e| error(set, Some(&entry.name), e))
        })?;
    }
    Ok(())
}

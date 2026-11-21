//! Post-processors for the archives in a finished job folder.
//!
//! [`SafetyCheck`] (`postproc-safety`) looks at every plain file and every archive entry before
//! anything is written, and fails the job on a blocked file type, a path that leaves the job
//! folder, a name no file system should get, or a link pointing outside. [`Extractor`]
//! (`postproc-extract`) then extracts RAR and 7z archives, multi-volume ones included.
//!
//! The extractor applies the path, name and link rules again on its own, so it never writes
//! outside the job folder even when the safety check is off. Blocked file types are the safety
//! check's alone: they are a policy, not a precondition for writing safely.
//!
//! Archives are untrusted input. Entries are written under a temporary name and only get their
//! own once their checksum matched; links are never created. A failed extraction removes what it
//! wrote and leaves the archives in place, so a retry with the right password starts clean.

use std::collections::HashSet;
use std::fmt;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use fangst_providers::{
    BoxFuture, JobContext, PostProcessOutcome, PostProcessor, ProviderError, ProviderInfo,
    ProviderKind,
};

pub mod names;
mod rar;
pub mod sets;
mod sevenz;

use names::{LinkTarget, Unsafe, check_link, entry_path, is_blocked};
use sets::{ArchiveSet, Format, PART_SUFFIX};

pub const SAFETY_ID: &str = "postproc-safety";
pub const EXTRACT_ID: &str = "postproc-extract";

/// rar-rs keeps large decoder state on the stack; the 1 MiB a Windows thread gets by default
/// overflowed in debug builds.
const WORKER_STACK_BYTES: usize = 8 << 20;

/// One member of an archive, as listed before extraction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Directory,
    Link(LinkTarget),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    PasswordMissing {
        archive: String,
    },
    PasswordWrong {
        archive: String,
    },
    /// `archive` is `None` for a file lying in the job folder itself.
    Unsafe {
        archive: Option<String>,
        entry: String,
        why: Unsafe,
    },
    Blocked {
        archive: Option<String>,
        file: String,
    },
    Damaged {
        archive: String,
        message: String,
    },
    Io {
        path: String,
        message: String,
    },
}

impl Error {
    fn io(path: &Path, e: &io::Error) -> Self {
        Self::Io {
            path: path.display().to_string(),
            message: e.to_string(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PasswordMissing { archive } => write!(
                f,
                "{archive} is password protected and the job has no password; add it to the NZB \
                 or end the job name with {{{{password}}}}"
            ),
            Self::PasswordWrong { archive } => {
                write!(f, "the job's password does not open {archive}")
            }
            Self::Unsafe {
                archive: Some(archive),
                entry,
                why,
            } => write!(f, "{archive} was refused: `{entry}` {why}"),
            Self::Unsafe {
                archive: None,
                entry,
                why,
            } => write!(f, "`{entry}` in the job folder {why}"),
            Self::Blocked {
                archive: Some(archive),
                file,
            } => write!(f, "{archive} contains `{file}`, a blocked file type"),
            Self::Blocked {
                archive: None,
                file,
            } => write!(f, "the job contains `{file}`, a blocked file type"),
            Self::Damaged { archive, message } => {
                write!(f, "{archive} could not be extracted: {message}")
            }
            Self::Io { path, message } => write!(f, "{path}: {message}"),
        }
    }
}

impl std::error::Error for Error {}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn relative(folder: &Path, path: &Path) -> String {
    path.strip_prefix(folder)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn list(set: &ArchiveSet, password: Option<&str>) -> Result<Vec<Entry>, Error> {
    match set.format {
        Format::Rar => rar::list(set, password),
        Format::SevenZip => sevenz::list(set, password),
    }
}

/// Applies the path, name and link rules to every entry, and the blocked types when given.
fn check_entries(
    set: &ArchiveSet,
    entries: &[Entry],
    blocked: Option<&[String]>,
) -> Result<(), Error> {
    for entry in entries {
        let refuse = |why| Error::Unsafe {
            archive: Some(set.name()),
            entry: entry.name.clone(),
            why,
        };
        let path = entry_path(&entry.name).map_err(refuse)?;
        if let EntryKind::Link(target) = &entry.kind {
            check_link(&path, target).map_err(refuse)?;
        }
        if let Some(blocked) = blocked
            && entry.kind != EntryKind::Directory
            && is_blocked(&entry.name, blocked)
        {
            return Err(Error::Blocked {
                archive: Some(set.name()),
                file: entry.name.clone(),
            });
        }
    }
    Ok(())
}

/// What [`inspect`] looked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Inspection {
    pub files: usize,
    pub archives: usize,
    pub entries: usize,
}

impl fmt::Display for Inspection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.archives == 0 {
            return write!(
                f,
                "checked {}, no archives",
                count(self.files, "file", "files")
            );
        }
        write!(
            f,
            "checked {} and {} with {}; nothing blocked",
            count(self.files, "file", "files"),
            count(self.archives, "archive", "archives"),
            count(self.entries, "entry", "entries")
        )
    }
}

/// Checks every file in `folder` and every entry of every archive there, writing nothing. Blocks
/// the thread.
///
/// # Errors
/// The first file or entry that breaks a rule, an archive that needs a password the job lacks,
/// or an archive that can't be listed.
pub fn inspect(
    folder: &Path,
    password: Option<&str>,
    blocked: &[String],
) -> Result<Inspection, Error> {
    let walk = sets::walk(folder).map_err(|e| Error::io(folder, &e))?;
    if let Some(link) = walk.links.first() {
        return Err(Error::Unsafe {
            archive: None,
            entry: relative(folder, link),
            why: Unsafe::LinkUnchecked,
        });
    }
    if let Some(file) = walk
        .files
        .iter()
        .find(|f| is_blocked(&f.to_string_lossy(), blocked))
    {
        return Err(Error::Blocked {
            archive: None,
            file: relative(folder, file),
        });
    }
    let archives = sets::find(&walk.files);
    let mut entries = 0;
    for set in &archives {
        let listed = list(set, password)?;
        check_entries(set, &listed, Some(blocked))?;
        entries += listed.len();
    }
    Ok(Inspection {
        files: walk.files.len(),
        archives: archives.len(),
        entries,
    })
}

/// Where extracted files go, and what was written so a failure can take it back.
pub(crate) struct Output {
    folder: PathBuf,
    written: Vec<PathBuf>,
}

impl Output {
    /// Writes one file through `copy`, under a temporary name until `copy` succeeds.
    fn file(
        &mut self,
        path: &Path,
        copy: impl FnOnce(&mut dyn Write) -> Result<(), Error>,
    ) -> Result<(), Error> {
        self.prepare_parent(path)?;
        let mut part = path.as_os_str().to_owned();
        part.push(PART_SUFFIX);
        let part = PathBuf::from(part);
        // A leftover from an interrupted run, or a link planted under this name, is removed rather
        // than opened: opening would follow the link.
        match std::fs::remove_file(&part) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(Error::io(&part, &e)),
            _ => {}
        }
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&part)
            .map_err(|e| Error::io(&part, &e))?;
        let mut file = io::BufWriter::new(file);
        let result = copy(&mut file).and_then(|()| file.flush().map_err(|e| Error::io(&part, &e)));
        drop(file);
        if let Err(e) = result {
            let _ = std::fs::remove_file(&part);
            return Err(e);
        }
        std::fs::rename(&part, path).map_err(|e| Error::io(path, &e))?;
        self.written.push(path.to_owned());
        Ok(())
    }

    /// Creates the folders above `path`, refusing to pass through a link someone planted there.
    fn prepare_parent(&self, path: &Path) -> Result<(), Error> {
        let Some(parent) = path.parent() else {
            return Ok(());
        };
        let below = parent.strip_prefix(&self.folder).unwrap_or(Path::new(""));
        let mut at = self.folder.clone();
        for part in below.components() {
            at.push(part);
            match std::fs::symlink_metadata(&at) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err(Error::Unsafe {
                        archive: None,
                        entry: relative(&self.folder, &at),
                        why: Unsafe::LinkUnchecked,
                    });
                }
                Ok(_) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    std::fs::create_dir(&at).map_err(|e| Error::io(&at, &e))?;
                }
                Err(e) => return Err(Error::io(&at, &e)),
            }
        }
        Ok(())
    }

    fn undo(&self) {
        for path in &self.written {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// One archive [`extract`] unpacked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extracted {
    pub archive: String,
    pub volumes: usize,
    pub files: usize,
    /// Link entries, which are never created.
    pub links_skipped: usize,
}

/// What [`extract`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Extraction {
    pub archives: Vec<Extracted>,
    /// Archive files removed after every archive extracted.
    pub deleted: usize,
    /// Archive files that could not be removed, with why. The job still succeeds.
    pub not_deleted: Vec<String>,
}

impl fmt::Display for Extraction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.archives.is_empty() {
            return f.write_str("no archives to extract");
        }
        for (i, a) in self.archives.iter().enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(
                f,
                "{} ({}): extracted {}",
                a.archive,
                count(a.volumes, "volume", "volumes"),
                count(a.files, "file", "files")
            )?;
            if a.links_skipped > 0 {
                write!(f, ", skipped {}", count(a.links_skipped, "link", "links"))?;
            }
        }
        if self.deleted > 0 {
            write!(
                f,
                "; deleted {}",
                count(self.deleted, "archive file", "archive files")
            )?;
        }
        for problem in &self.not_deleted {
            write!(f, "; could not delete {problem}")?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractSettings {
    /// Remove the archive files once every archive in the job extracted.
    pub delete_archives: bool,
}

fn extract_set(
    set: &ArchiveSet,
    password: Option<&str>,
    archive_files: &HashSet<String>,
    out: &mut Output,
) -> Result<Extracted, Error> {
    let entries = list(set, password)?;
    check_entries(set, &entries, None)?;
    let folder = set.folder().to_owned();
    let target = |entry: &Entry| match entry.kind {
        EntryKind::File => entry_path(&entry.name).ok().map(|p| folder.join(p)),
        EntryKind::Directory | EntryKind::Link(_) => None,
    };
    let mut files = 0;
    for entry in &entries {
        if let Some(path) = target(entry) {
            if archive_files.contains(&path.to_string_lossy().to_lowercase()) {
                return Err(Error::Damaged {
                    archive: set.name(),
                    message: format!("`{}` would overwrite one of the job's archives", entry.name),
                });
            }
            files += 1;
        }
    }
    match set.format {
        Format::Rar => rar::extract(set, password, &target, out)?,
        Format::SevenZip => sevenz::extract(set, password, &target, out)?,
    }
    Ok(Extracted {
        archive: set.name(),
        volumes: set.volumes.len(),
        files,
        links_skipped: entries
            .iter()
            .filter(|e| matches!(e.kind, EntryKind::Link(_)))
            .count(),
    })
}

/// Extracts every RAR and 7z archive in `folder` into the folder it lies in. Blocks the thread.
///
/// # Errors
/// An archive that needs a password the job lacks or that the password doesn't open, an entry
/// that breaks the path, name or link rules, a damaged archive, or a write that fails. Whatever
/// this call wrote is removed again and the archives stay.
pub fn extract(
    folder: &Path,
    password: Option<&str>,
    settings: &ExtractSettings,
) -> Result<Extraction, Error> {
    let walk = sets::walk(folder).map_err(|e| Error::io(folder, &e))?;
    let archives = sets::find(&walk.files);
    // Lowercased, since Windows and macOS treat names that differ only in case as one file.
    let archive_files: HashSet<String> = archives
        .iter()
        .flat_map(|s| &s.volumes)
        .map(|p| p.to_string_lossy().to_lowercase())
        .collect();
    let mut out = Output {
        folder: folder.to_owned(),
        written: Vec::new(),
    };
    let mut extraction = Extraction::default();
    for set in &archives {
        match extract_set(set, password, &archive_files, &mut out) {
            Ok(done) => extraction.archives.push(done),
            Err(e) => {
                out.undo();
                return Err(e);
            }
        }
    }
    if settings.delete_archives {
        for volume in archives.iter().flat_map(|s| &s.volumes) {
            match std::fs::remove_file(volume) {
                Ok(()) => extraction.deleted += 1,
                Err(e) => extraction
                    .not_deleted
                    .push(format!("{}: {e}", relative(folder, volume))),
            }
        }
    }
    Ok(extraction)
}

/// Runs `work` on its own thread with room for the RAR decoder, without holding up the runtime.
fn on_worker<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> BoxFuture<Result<T, ProviderError>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let spawned = std::thread::Builder::new()
        .name("fangst-extract".to_owned())
        .stack_size(WORKER_STACK_BYTES)
        .spawn(move || {
            let _ = tx.send(work());
        });
    Box::pin(async move {
        spawned.map_err(|e| {
            ProviderError::new(format!("could not start the archive thread: {e}"), true)
        })?;
        // A panic drops the sender, so it ends up here instead of taking the daemon down.
        rx.await
            .map_err(|_| ProviderError::new("the archive step stopped unexpectedly", false))
    })
}

fn outcome<T: fmt::Display>(result: Result<T, Error>) -> PostProcessOutcome {
    match result {
        Ok(done) => PostProcessOutcome::Continue {
            message: done.to_string(),
        },
        Err(e) => PostProcessOutcome::Fail {
            reason: e.to_string(),
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafetySettings {
    /// Extensions, without the dot, that fail a job wherever they appear.
    pub blocked_extensions: Vec<String>,
}

/// The `postproc-safety` provider: [`inspect`] as a post-processor.
pub struct SafetyCheck {
    settings: Arc<SafetySettings>,
}

impl SafetyCheck {
    #[must_use]
    pub fn new(settings: SafetySettings) -> Self {
        Self {
            settings: Arc::new(settings),
        }
    }
}

impl PostProcessor for SafetyCheck {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: SAFETY_ID.to_owned(),
            kind: ProviderKind::PostProcessor,
            description: "Fails a job holding blocked file types, or archives with paths or links \
                          leading out of the job folder"
                .to_owned(),
        }
    }

    fn process(&self, job: JobContext) -> BoxFuture<Result<PostProcessOutcome, ProviderError>> {
        let settings = Arc::clone(&self.settings);
        let work = on_worker(move || {
            outcome(inspect(
                &job.folder,
                job.password.as_deref(),
                &settings.blocked_extensions,
            ))
        });
        Box::pin(work)
    }
}

/// The `postproc-extract` provider: [`extract`] as a post-processor.
pub struct Extractor {
    settings: Arc<ExtractSettings>,
}

impl Extractor {
    #[must_use]
    pub fn new(settings: ExtractSettings) -> Self {
        Self {
            settings: Arc::new(settings),
        }
    }
}

impl PostProcessor for Extractor {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: EXTRACT_ID.to_owned(),
            kind: ProviderKind::PostProcessor,
            description: "Extracts RAR and 7z archives, with the password from the NZB or the job"
                .to_owned(),
        }
    }

    fn process(&self, job: JobContext) -> BoxFuture<Result<PostProcessOutcome, ProviderError>> {
        let settings = Arc::clone(&self.settings);
        Box::pin(on_worker(move || {
            outcome(extract(&job.folder, job.password.as_deref(), &settings))
        }))
    }
}

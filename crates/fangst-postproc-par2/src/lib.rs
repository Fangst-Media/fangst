//! PAR2 verify and repair for a job folder.
//!
//! A post usually carries one small index `.par2` with checksums only, plus recovery volumes named
//! `<base>.volNN+MM.par2` holding `MM` recovery blocks each. The caller downloads the data and the
//! index first, runs [`verify`], fetches the volumes [`choose_volumes`] picks for the blocks
//! needed, then runs [`repair`]. Verify and repair read whole files and block the thread; call
//! them from a blocking task.
//!
//! PAR2 files are untrusted input: a set that names a file outside the folder is refused before
//! anything is read or written.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

/// Beyond this a block size is broken or hostile: repair holds several blocks in memory at once.
/// Real posts use blocks well under 100 MiB.
const MAX_SLICE_SIZE: u64 = 1 << 30;

/// The PAR2 recovery exponent is 16 bits, so no volume holds more blocks than this.
const MAX_VOLUME_BLOCKS: u32 = 1 << 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The PAR2 file could not be read or holds no usable set.
    Unreadable(String),
    /// The set names a file that would land outside the job folder.
    UnsafeName(String),
    /// The set's own description is impossible, such as a zero block size.
    InvalidSet(String),
    NotEnoughRecovery {
        needed: u32,
        available: u32,
    },
    Repair(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(e) => write!(f, "could not read the PAR2 file: {e}"),
            Self::UnsafeName(name) => {
                write!(
                    f,
                    "the PAR2 set names `{name}`, which is not a plain file name"
                )
            }
            Self::InvalidSet(e) => write!(f, "the PAR2 set is invalid: {e}"),
            Self::NotEnoughRecovery { needed, available } => write!(
                f,
                "PAR2 repair needs {needed} recovery blocks, only {available} are available"
            ),
            Self::Repair(e) => write!(f, "PAR2 repair failed: {e}"),
        }
    }
}

impl std::error::Error for Error {}

/// A recovery volume, as its file name describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub name: String,
    /// Exponent of the first recovery block.
    pub first: u32,
    pub blocks: u32,
}

/// The PAR2 files that share a base name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Set {
    /// The `.par2` file without recovery blocks, if the post has one.
    pub index: Option<String>,
    /// Ordered by first block.
    pub volumes: Vec<Volume>,
}

/// Whether `name` is any PAR2 file, index or volume.
#[must_use]
pub fn is_par2(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".par2")
}

/// Reads `<base>.volNN+MM.par2` (some posters write `-` instead of `+`) into its base and volume.
#[must_use]
pub fn parse_volume(name: &str) -> Option<(String, Volume)> {
    let stem = strip_par2(name)?;
    let lower = stem.to_ascii_lowercase();
    let at = lower.rfind(".vol")?;
    let (first, blocks) = lower[at + 4..].split_once(['+', '-'])?;
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(first) || !all_digits(blocks) {
        return None;
    }
    let first: u32 = first.parse().ok()?;
    let blocks: u32 = blocks.parse().ok()?;
    if blocks == 0 || blocks > MAX_VOLUME_BLOCKS {
        return None;
    }
    Some((
        stem[..at].to_owned(),
        Volume {
            name: name.to_owned(),
            first,
            blocks,
        },
    ))
}

fn strip_par2(name: &str) -> Option<&str> {
    let cut = name.len().checked_sub(".par2".len())?;
    (name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(".par2")).then(|| &name[..cut])
}

/// Groups PAR2 file names into sets by base name, ignoring case. Other names are skipped.
#[must_use]
pub fn sets<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<Set> {
    let mut by_base: BTreeMap<String, Set> = BTreeMap::new();
    for name in names {
        let Some(stem) = strip_par2(name) else {
            continue;
        };
        let (base, volume) = match parse_volume(name) {
            Some((base, volume)) => (base, Some(volume)),
            None => (stem.to_owned(), None),
        };
        let set = by_base
            .entry(base.to_ascii_lowercase())
            .or_insert_with(|| Set {
                index: None,
                volumes: Vec::new(),
            });
        match volume {
            Some(volume) => set.volumes.push(volume),
            None if set.index.is_none() => set.index = Some(name.to_owned()),
            None => {}
        }
    }
    let mut sets: Vec<Set> = by_base.into_values().collect();
    for set in &mut sets {
        set.volumes.sort_by_key(|v| v.first);
    }
    sets
}

/// Indices into `volumes` whose blocks add up to at least `needed` with the fewest blocks in
/// total, so the least data is downloaded. `None` when all of them together are too few.
#[must_use]
pub fn choose_volumes(volumes: &[Volume], needed: u32) -> Option<Vec<usize>> {
    let total: u64 = volumes.iter().map(|v| u64::from(v.blocks)).sum();
    if total < u64::from(needed) {
        return None;
    }
    if needed == 0 {
        return Some(Vec::new());
    }
    // An optimal pick never reaches `needed + largest`: dropping any volume would still cover it.
    let largest = volumes.iter().map(|v| v.blocks).max().unwrap_or(0);
    let limit = usize::try_from(needed.saturating_add(largest)).ok()?;
    // reached[sum] = (previous sum, volume added), filled as a 0/1 knapsack.
    let mut reached: Vec<Option<(usize, usize)>> = vec![None; limit + 1];
    let mut start = vec![false; limit + 1];
    start[0] = true;
    for (i, volume) in volumes.iter().enumerate() {
        let blocks = usize::try_from(volume.blocks).ok()?;
        for sum in (blocks..=limit).rev() {
            let from = sum - blocks;
            if !start[sum] && start[from] {
                start[sum] = true;
                reached[sum] = Some((from, i));
            }
        }
    }
    let needed = usize::try_from(needed).ok()?;
    let mut sum = (needed..=limit).find(|&s| start[s])?;
    let mut picked = Vec::new();
    while let Some((from, i)) = reached[sum] {
        picked.push(i);
        sum = from;
    }
    picked.sort_unstable();
    Some(picked)
}

/// What [`verify`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    /// Every file the set covers.
    pub files: Vec<String>,
    /// Files that exist with damage, and how many blocks of each are damaged, by name.
    pub damaged: Vec<(String, u32)>,
    /// Files that don't exist, and how many blocks they span, by name.
    pub missing: Vec<(String, u32)>,
    /// Recovery blocks a repair needs.
    pub blocks_needed: u32,
}

impl Verification {
    #[must_use]
    pub fn is_intact(&self) -> bool {
        self.damaged.is_empty() && self.missing.is_empty()
    }
}

impl fmt::Display for Verification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_intact() {
            return write!(f, "all {} files intact", self.files.len());
        }
        let parts: Vec<String> = self
            .damaged
            .iter()
            .map(|(name, blocks)| format!("{name} has {blocks} damaged blocks"))
            .chain(
                self.missing
                    .iter()
                    .map(|(name, blocks)| format!("{name} is missing ({blocks} blocks)")),
            )
            .collect();
        write!(
            f,
            "{}; {} recovery blocks needed",
            parts.join(", "),
            self.blocks_needed
        )
    }
}

/// What [`repair`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repaired {
    pub blocks: u32,
    pub files: usize,
}

/// Checks the files in `folder` against the set described by `par2_file`. Blocks the thread.
///
/// # Errors
/// When the PAR2 file can't be read or describes an unsafe or impossible set.
pub fn verify(folder: &Path, par2_file: &Path) -> Result<Verification, Error> {
    let set = load(par2_file)?;
    Ok(summarize(&set, &rust_par2::verify(&set, folder)))
}

/// Repairs the files in `folder` with the recovery blocks of every `.par2` file there that belongs
/// to the set `par2_file` describes. Blocks the thread.
///
/// # Errors
/// [`Error::NotEnoughRecovery`] when the folder holds too few recovery blocks; nothing is written
/// then. Otherwise when the set can't be read or the repaired files still don't match.
pub fn repair(folder: &Path, par2_file: &Path) -> Result<Repaired, Error> {
    let set = load(par2_file)?;
    let verified = rust_par2::verify(&set, folder);
    if verified.all_correct() {
        return Ok(Repaired {
            blocks: 0,
            files: 0,
        });
    }
    match rust_par2::repair_from_verify(&set, folder, &verified) {
        Ok(result) if result.success => Ok(Repaired {
            blocks: result.blocks_repaired,
            files: result.files_repaired,
        }),
        Ok(result) => Err(Error::Repair(result.message)),
        Err(rust_par2::RepairError::InsufficientRecovery { needed, available }) => {
            Err(Error::NotEnoughRecovery { needed, available })
        }
        Err(e) => Err(Error::Repair(e.to_string())),
    }
}

fn load(par2_file: &Path) -> Result<rust_par2::Par2FileSet, Error> {
    let set = rust_par2::parse(par2_file).map_err(|e| Error::Unreadable(e.to_string()))?;
    if set.files.is_empty() {
        return Err(Error::Unreadable("it describes no files".to_owned()));
    }
    if set.slice_size == 0 || !set.slice_size.is_multiple_of(4) || set.slice_size > MAX_SLICE_SIZE {
        return Err(Error::InvalidSet(format!(
            "block size {} is not usable",
            set.slice_size
        )));
    }
    let mut files: Vec<&rust_par2::Par2File> = set.files.values().collect();
    files.sort_by(|a, b| a.filename.cmp(&b.filename));
    // rust-par2 joins these names onto the folder and writes repaired blocks there.
    if let Some(file) = files.iter().find(|f| {
        fangst_core::path::safe_file_name(&f.filename).as_deref() != Some(f.filename.as_str())
    }) {
        return Err(Error::UnsafeName(file.filename.clone()));
    }
    Ok(set)
}

fn summarize(set: &rust_par2::Par2FileSet, result: &rust_par2::VerifyResult) -> Verification {
    let mut files: Vec<String> = set.files.values().map(|f| f.filename.clone()).collect();
    files.sort();
    let mut damaged: Vec<(String, u32)> = result
        .damaged
        .iter()
        .map(|d| (d.filename.clone(), d.damaged_block_count))
        .collect();
    damaged.sort();
    let mut missing: Vec<(String, u32)> = result
        .missing
        .iter()
        .map(|m| (m.filename.clone(), m.block_count))
        .collect();
    missing.sort();
    Verification {
        files,
        damaged,
        missing,
        blocks_needed: result.blocks_needed(),
    }
}

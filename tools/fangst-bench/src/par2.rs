//! Creates PAR2 sets for tests, laid out the way posters upload them: one small index file with
//! only the checksums, then recovery volumes named `<base>.volNN+MM.par2`.
//!
//! `rust-par2` only verifies and repairs, so the encoder lives here. It uses that crate's field
//! arithmetic and slice constants, so a set that repairs proves both sides agree with the spec.

use md5::{Digest, Md5};
use rust_par2::{SliceHasher, gf, matrix::par2_input_constants};

const MAGIC: &[u8; 8] = b"PAR2\0PKT";
const TYPE_MAIN: &[u8; 16] = b"PAR 2.0\0Main\0\0\0\0";
const TYPE_FILE_DESC: &[u8; 16] = b"PAR 2.0\0FileDesc";
const TYPE_IFSC: &[u8; 16] = b"PAR 2.0\0IFSC\0\0\0\0";
const TYPE_RECOVERY: &[u8; 16] = b"PAR 2.0\0RecvSlic";
const TYPE_CREATOR: &[u8; 16] = b"PAR 2.0\0Creator\0";
const HASH_16K: usize = 16 * 1024;

struct Source<'a> {
    id: [u8; 16],
    name: &'a str,
    data: &'a [u8],
}

/// Returns `(file name, contents)` for the index file and one volume per entry of
/// `volume_blocks`, each holding that many recovery blocks.
///
/// # Panics
/// When `slice_size` is not a positive multiple of 4, as PAR2 requires.
#[must_use]
pub fn create(
    base: &str,
    files: &[(&str, &[u8])],
    slice_size: usize,
    volume_blocks: &[u32],
) -> Vec<(String, Vec<u8>)> {
    assert!(
        slice_size > 0 && slice_size.is_multiple_of(4),
        "slice size must be a positive multiple of 4"
    );
    let mut sources: Vec<Source<'_>> = files
        .iter()
        .map(|(name, data)| Source {
            id: file_id(name, data),
            name,
            data,
        })
        .collect();
    // The Main packet lists files by ascending id, and the recovery math follows that order.
    sources.sort_by_key(|s| s.id);

    let mut main = (slice_size as u64).to_le_bytes().to_vec();
    main.extend_from_slice(
        &u32::try_from(sources.len())
            .expect("file count")
            .to_le_bytes(),
    );
    for source in &sources {
        main.extend_from_slice(&source.id);
    }
    let set_id: [u8; 16] = Md5::digest(&main).into();

    let mut critical = packet(&set_id, TYPE_MAIN, &main);
    for source in &sources {
        critical.extend(packet(&set_id, TYPE_FILE_DESC, &file_desc(source)));
        critical.extend(packet(&set_id, TYPE_IFSC, &ifsc(source, slice_size)));
    }
    critical.extend(packet(&set_id, TYPE_CREATOR, &padded(b"fangst-bench")));

    let slices: Vec<Vec<u8>> = sources
        .iter()
        .flat_map(|s| s.data.chunks(slice_size))
        .map(|chunk| {
            let mut slice = chunk.to_vec();
            slice.resize(slice_size, 0);
            slice
        })
        .collect();
    let constants = par2_input_constants(slices.len());

    let mut out = vec![(format!("{base}.par2"), critical.clone())];
    let mut exponent = 0u32;
    for &count in volume_blocks {
        let mut volume = Vec::new();
        for e in exponent..exponent + count {
            let mut body = e.to_le_bytes().to_vec();
            body.extend(recovery_block(&slices, &constants, e, slice_size));
            volume.extend(packet(&set_id, TYPE_RECOVERY, &body));
        }
        volume.extend_from_slice(&critical);
        out.push((format!("{base}.vol{exponent:02}+{count:02}.par2"), volume));
        exponent += count;
    }
    out
}

fn recovery_block(slices: &[Vec<u8>], constants: &[u16], exponent: u32, size: usize) -> Vec<u8> {
    let mut words = vec![0u16; size / 2];
    for (slice, &constant) in slices.iter().zip(constants) {
        let factor = gf::pow(constant, exponent);
        for (acc, pair) in words.iter_mut().zip(slice.as_chunks::<2>().0) {
            *acc ^= gf::mul(factor, u16::from_le_bytes(*pair));
        }
    }
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

fn file_id(name: &str, data: &[u8]) -> [u8; 16] {
    let mut hasher = Md5::new();
    hasher.update(Md5::digest(&data[..data.len().min(HASH_16K)]));
    hasher.update((data.len() as u64).to_le_bytes());
    hasher.update(name.as_bytes());
    hasher.finalize().into()
}

fn file_desc(source: &Source<'_>) -> Vec<u8> {
    let mut body = source.id.to_vec();
    body.extend_from_slice(&Md5::digest(source.data));
    body.extend_from_slice(&Md5::digest(
        &source.data[..source.data.len().min(HASH_16K)],
    ));
    body.extend_from_slice(&(source.data.len() as u64).to_le_bytes());
    body.extend(padded(source.name.as_bytes()));
    body
}

fn ifsc(source: &Source<'_>, slice_size: usize) -> Vec<u8> {
    let mut body = source.id.to_vec();
    for chunk in source.data.chunks(slice_size) {
        let checksum = SliceHasher::checksum(chunk, slice_size as u64);
        body.extend_from_slice(&checksum.md5);
        body.extend_from_slice(&checksum.crc32.to_le_bytes());
    }
    body
}

fn padded(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    out.resize(bytes.len().next_multiple_of(4), 0);
    out
}

fn packet(set_id: &[u8; 16], kind: &[u8; 16], body: &[u8]) -> Vec<u8> {
    let mut hashed = set_id.to_vec();
    hashed.extend_from_slice(kind);
    hashed.extend_from_slice(body);
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&(32 + hashed.len() as u64).to_le_bytes());
    out.extend_from_slice(&Md5::digest(&hashed));
    out.extend(hashed);
    out
}

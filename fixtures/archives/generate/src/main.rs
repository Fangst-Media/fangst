//! Writes every archive under `fixtures/archives` from scratch:
//! `cargo run --manifest-path fixtures/archives/generate/Cargo.toml -- fixtures/archives`.
//!
//! RAR archives come from `rar-rs`, an open BSD-2-Clause RAR writer, and 7z archives from
//! `sevenz-rust2`. Contents are generated, so the tests can rebuild the expected bytes with the
//! same `payload` function.

use std::io::{Cursor, Read};
use std::path::Path;

use rar_rs::{ArchiveVersion, ArchiveWriter, CompressionLevel, EntryWriteOptions, WriterOptions};
use sevenz_rust2::encoder_options::AesEncoderOptions;
use sevenz_rust2::{ArchiveEntry, EncoderMethod};

/// The password of every protected fixture.
const PASSWORD: &str = "fangst-test";
/// Sized so a payload spans three volumes.
const PAYLOAD_LEN: usize = 20_000;
const VOLUME_SIZE: usize = 8192;

/// The same bytes the tests expect: an LCG, so they don't compress and need no file of their own.
fn payload(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed;
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            x.to_be_bytes()[0]
        })
        .collect()
}

fn store() -> EntryWriteOptions {
    EntryWriteOptions::new().compression_level(CompressionLevel::STORE)
}

fn rar(path: &Path, options: WriterOptions, members: &[(&str, &[u8])]) {
    let mut writer = ArchiveWriter::create_with(path, options).unwrap();
    for (name, data) in members {
        writer.add_bytes(name, data, store()).unwrap();
    }
    writer.finish().unwrap();
}

/// A 7z archive in memory; `password` encrypts data and names.
fn seven_zip(members: &[(ArchiveEntry, &[u8])], password: Option<&str>) -> Vec<u8> {
    let mut writer = sevenz_rust2::ArchiveWriter::new(Cursor::new(Vec::new())).unwrap();
    if let Some(password) = password {
        writer.set_content_methods(vec![
            AesEncoderOptions::new(password.into()).into(),
            EncoderMethod::LZMA2.into(),
        ]);
    }
    for (entry, data) in members {
        let reader: Option<&[u8]> = (!entry.is_directory).then_some(*data);
        writer.push_archive_entry(entry.clone(), reader).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn file(name: &str) -> ArchiveEntry {
    ArchiveEntry::new_file(name)
}

/// A Unix symlink as 7-Zip stores one: the mode in the high 16 bits, the target as content.
fn symlink(name: &str) -> ArchiveEntry {
    let mut entry = ArchiveEntry::new_file(name);
    entry.has_windows_attributes = true;
    entry.windows_attributes = 0x8000 | (0o120_777 << 16);
    entry
}

fn write(path: &Path, bytes: &[u8]) {
    std::fs::write(path, bytes).unwrap();
}

fn main() {
    // rar-rs overflows the 1 MiB main-thread stack Windows gives a debug build when it encrypts.
    std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(generate)
        .unwrap()
        .join()
        .unwrap();
}

fn generate() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("output folder"));
    let show = payload(PAYLOAD_LEN, 1);
    let small = payload(3000, 2);
    let dir = |name: &str| {
        let path = root.join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    };

    // RAR5 split the way current WinRAR names volumes: show.part1.rar, show.part2.rar, ...
    rar(
        &dir("rar5-volumes").join("show.rar"),
        WriterOptions::new().volume_size(VOLUME_SIZE as u64),
        &[("show.mkv", &show)],
    );
    // RAR4 with the old naming many posts still use: show.rar, show.r00, show.r01.
    rar(
        &dir("rar4-old-naming").join("show.rar"),
        WriterOptions::new()
            .compression(ArchiveVersion::V29)
            .volume_size(VOLUME_SIZE as u64)
            .old_numbering(true),
        &[("show.mkv", &show), ("Sample/show-sample.mkv", &small)],
    );
    rar(
        &dir("rar5-password").join("secret.rar"),
        WriterOptions::new().password(PASSWORD),
        &[("secret.mkv", &small)],
    );
    rar(
        &dir("rar5-header-password").join("hidden.rar"),
        WriterOptions::new().password(PASSWORD).encrypt_headers(true),
        &[("hidden.mkv", &small)],
    );

    write(
        &dir("7z").join("show.7z"),
        &seven_zip(
            &[(file("show.mkv"), &show), (file("Sample/show-sample.mkv"), &small)],
            None,
        ),
    );
    // 7-Zip splits a finished archive into plain byte ranges named .7z.001, .7z.002, ...
    let split = seven_zip(&[(file("show.mkv"), &show)], None);
    let volumes = dir("7z-volumes");
    for (i, part) in split.chunks(VOLUME_SIZE).enumerate() {
        write(&volumes.join(format!("show.7z.{:03}", i + 1)), part);
    }
    write(
        &dir("7z-password").join("secret.7z"),
        &seven_zip(&[(file("secret.mkv"), &small)], Some(PASSWORD)),
    );

    let bad = dir("malicious");
    let evil: &[u8] = b"not a real executable";
    rar(
        &bad.join("zip-slip.rar"),
        WriterOptions::new(),
        &[("show.mkv", &small), ("../evil.txt", evil)],
    );
    rar(
        &bad.join("absolute.rar"),
        WriterOptions::new(),
        &[("/tmp/evil.txt", evil)],
    );
    rar(
        &bad.join("executable.rar"),
        WriterOptions::new(),
        &[("show.mkv", &small), ("setup.exe", evil)],
    );
    rar(
        &bad.join("reserved-name.rar"),
        WriterOptions::new(),
        &[("CON.mkv", &small)],
    );
    let mut writer = ArchiveWriter::create(bad.join("symlink.rar")).unwrap();
    writer.add_bytes("show.mkv", &small, store()).unwrap();
    // Redirect type 1 is a Unix symlink.
    writer.add_redirect("escape", 1, "../../outside").unwrap();
    writer.finish().unwrap();

    write(
        &bad.join("zip-slip.7z"),
        &seven_zip(
            &[(file("show.mkv"), &small), (file("Sample/../../evil.txt"), evil)],
            None,
        ),
    );
    write(
        &bad.join("drive.7z"),
        &seven_zip(&[(file("C:\\Windows\\evil.txt"), evil)], None),
    );
    write(
        &bad.join("shortcut.7z"),
        &seven_zip(&[(file("show.mkv"), &small), (file("Watch Now.lnk"), evil)], None),
    );
    write(
        &bad.join("symlink.7z"),
        &seven_zip(&[(symlink("escape"), b"../../outside")], None),
    );

    check(&root);
}

/// Lists what was written, through the same libraries the extractor uses, as a sanity check.
fn check(root: &Path) {
    for set in std::fs::read_dir(root).unwrap() {
        let set = set.unwrap().path();
        if !set.is_dir() || set.ends_with("generate") {
            continue;
        }
        let mut names: Vec<_> = std::fs::read_dir(&set)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        names.sort();
        for path in names {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let line = if name.ends_with(".rar") {
                match rar_rs::ArchiveReader::open_with(
                    &path,
                    rar_rs::OpenOptions::new().password(PASSWORD),
                ) {
                    Ok(reader) => reader
                        .entries()
                        .map(|e| e.metadata().name().to_owned())
                        .collect::<Vec<_>>()
                        .join(", "),
                    Err(e) => format!("error: {e}"),
                }
            } else if name.ends_with(".7z") {
                let mut bytes = Vec::new();
                std::fs::File::open(&path)
                    .unwrap()
                    .read_to_end(&mut bytes)
                    .unwrap();
                match sevenz_rust2::Archive::read(&mut Cursor::new(bytes), &PASSWORD.into()) {
                    Ok(archive) => archive
                        .files
                        .iter()
                        .map(|f| f.name.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                    Err(e) => format!("error: {e}"),
                }
            } else {
                continue;
            };
            println!("{}/{name}: {line}", set.file_name().unwrap().to_string_lossy());
        }
    }
}

//! Extraction and safety checks over the archives in `fixtures/archives`. The generator there
//! documents how each one was made; `payload` rebuilds the bytes it put in them.

use std::path::{Path, PathBuf};

use fangst_postproc_extract::names::Unsafe;
use fangst_postproc_extract::{
    Error, ExtractSettings, Extractor, SafetyCheck, SafetySettings, extract, inspect,
};
use fangst_providers::{JobContext, PostProcessOutcome, PostProcessor};

const PASSWORD: &str = "fangst-test";

fn payload(len: usize, seed: u32) -> Vec<u8> {
    let mut x = seed;
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            x.to_be_bytes()[0]
        })
        .collect()
}

fn show() -> Vec<u8> {
    payload(20_000, 1)
}

fn small() -> Vec<u8> {
    payload(3000, 2)
}

fn fixtures() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    Path::new(&manifest).join("../../fixtures/archives")
}

/// A job folder holding copies of the named fixture files, as a download would leave them.
fn job(files: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for file in files {
        let from = fixtures().join(file);
        let name = from.file_name().unwrap().to_owned();
        std::fs::copy(&from, dir.path().join(name)).unwrap();
    }
    dir
}

fn names_in(folder: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let mut pending = vec![folder.to_owned()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let relative = path.strip_prefix(folder).unwrap();
                names.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    names.sort();
    names
}

const RAR5_VOLUMES: [&str; 3] = [
    "rar5-volumes/show.part1.rar",
    "rar5-volumes/show.part2.rar",
    "rar5-volumes/show.part3.rar",
];
const RAR4_VOLUMES: [&str; 3] = [
    "rar4-old-naming/show.rar",
    "rar4-old-naming/show.r00",
    "rar4-old-naming/show.r01",
];
const SEVEN_ZIP_VOLUMES: [&str; 3] = [
    "7z-volumes/show.7z.001",
    "7z-volumes/show.7z.002",
    "7z-volumes/show.7z.003",
];

fn delete() -> ExtractSettings {
    ExtractSettings {
        delete_archives: true,
    }
}

fn keep() -> ExtractSettings {
    ExtractSettings {
        delete_archives: false,
    }
}

fn blocked() -> Vec<String> {
    let defaults = fangst_core::defaults::Defaults::builtin().unwrap();
    defaults.safety.blocked_extensions
}

#[test]
fn extract_rar5_volumes_and_delete_them() {
    let dir = job(&RAR5_VOLUMES);

    let done = extract(dir.path(), None, &delete()).unwrap();

    assert_eq!(std::fs::read(dir.path().join("show.mkv")).unwrap(), show());
    assert_eq!(names_in(dir.path()), ["show.mkv"]);
    assert_eq!(
        done.to_string(),
        "show.part1.rar (3 volumes): extracted 1 file; deleted 3 archive files"
    );
}

#[test]
fn extract_rar4_old_naming_into_subfolders() {
    let dir = job(&RAR4_VOLUMES);

    extract(dir.path(), None, &delete()).unwrap();

    assert_eq!(names_in(dir.path()), ["Sample/show-sample.mkv", "show.mkv"]);
    assert_eq!(std::fs::read(dir.path().join("show.mkv")).unwrap(), show());
    assert_eq!(
        std::fs::read(dir.path().join("Sample/show-sample.mkv")).unwrap(),
        small()
    );
}

#[test]
fn extract_7z_archive_and_split_volumes() {
    let dir = job(&["7z/show.7z"]);
    extract(dir.path(), None, &delete()).unwrap();
    assert_eq!(names_in(dir.path()), ["Sample/show-sample.mkv", "show.mkv"]);
    assert_eq!(std::fs::read(dir.path().join("show.mkv")).unwrap(), show());

    let dir = job(&SEVEN_ZIP_VOLUMES);
    let done = extract(dir.path(), None, &delete()).unwrap();
    assert_eq!(names_in(dir.path()), ["show.mkv"]);
    assert_eq!(std::fs::read(dir.path().join("show.mkv")).unwrap(), show());
    assert!(
        done.to_string().starts_with("show.7z.001 (3 volumes)"),
        "{done}"
    );
}

#[test]
fn extract_replaces_a_leftover_from_an_interrupted_run() {
    let dir = job(&RAR5_VOLUMES);
    std::fs::write(dir.path().join("show.mkv.fangst-part"), b"half written").unwrap();
    std::fs::write(dir.path().join("show.mkv"), b"old").unwrap();

    extract(dir.path(), None, &delete()).unwrap();

    assert_eq!(names_in(dir.path()), ["show.mkv"]);
    assert_eq!(std::fs::read(dir.path().join("show.mkv")).unwrap(), show());
}

#[test]
fn extract_keeps_archives_when_told_to() {
    let dir = job(&RAR5_VOLUMES);

    let done = extract(dir.path(), None, &keep()).unwrap();

    assert_eq!(names_in(dir.path()).len(), 4);
    assert_eq!(done.deleted, 0);
}

#[test]
fn extract_with_the_password_opens_every_kind_of_protection() {
    for (fixture, output) in [
        ("rar5-password/secret.rar", "secret.mkv"),
        ("rar5-header-password/hidden.rar", "hidden.mkv"),
        ("7z-password/secret.7z", "secret.mkv"),
    ] {
        let dir = job(&[fixture]);
        extract(dir.path(), Some(PASSWORD), &delete()).unwrap();
        assert_eq!(
            std::fs::read(dir.path().join(output)).unwrap(),
            small(),
            "{fixture}"
        );
    }
}

#[test]
fn extract_without_a_password_fails_fast_and_writes_nothing() {
    for (fixture, archive) in [
        ("rar5-password/secret.rar", "secret.rar"),
        ("rar5-header-password/hidden.rar", "hidden.rar"),
        ("7z-password/secret.7z", "secret.7z"),
    ] {
        let dir = job(&[fixture]);
        let err = extract(dir.path(), None, &delete()).unwrap_err();
        assert_eq!(
            err,
            Error::PasswordMissing {
                archive: archive.to_owned()
            },
            "{fixture}"
        );
        assert!(err.to_string().contains("password protected"), "{err}");
        assert_eq!(names_in(dir.path()), [archive], "{fixture}");
    }
}

#[test]
fn extract_with_a_wrong_password_says_so() {
    for (fixture, archive) in [
        ("rar5-password/secret.rar", "secret.rar"),
        ("rar5-header-password/hidden.rar", "hidden.rar"),
        ("7z-password/secret.7z", "secret.7z"),
    ] {
        let dir = job(&[fixture]);
        let err = extract(dir.path(), Some("not it"), &delete()).unwrap_err();
        assert_eq!(
            err,
            Error::PasswordWrong {
                archive: archive.to_owned()
            },
            "{fixture}"
        );
        assert_eq!(names_in(dir.path()), [archive], "{fixture}");
    }
}

#[test]
fn extract_failure_removes_what_it_wrote_and_keeps_the_archives() {
    let mut files = RAR5_VOLUMES.to_vec();
    files.push("7z-password/secret.7z");
    let dir = job(&files);

    let err = extract(dir.path(), None, &delete()).unwrap_err();

    assert!(matches!(err, Error::PasswordMissing { .. }), "{err}");
    assert_eq!(
        names_in(dir.path()),
        [
            "secret.7z",
            "show.part1.rar",
            "show.part2.rar",
            "show.part3.rar"
        ]
    );
}

#[test]
fn extract_refuses_unsafe_entries_on_its_own() {
    for (fixture, entry, why) in [
        (
            "malicious/zip-slip.rar",
            "../evil.txt",
            Unsafe::ParentDirectory,
        ),
        (
            "malicious/zip-slip.7z",
            "Sample/../../evil.txt",
            Unsafe::ParentDirectory,
        ),
        ("malicious/absolute.rar", "/tmp/evil.txt", Unsafe::Absolute),
        (
            "malicious/drive.7z",
            "C:\\Windows\\evil.txt",
            Unsafe::Absolute,
        ),
        (
            "malicious/reserved-name.rar",
            "CON.mkv",
            Unsafe::InvalidName,
        ),
        ("malicious/symlink.rar", "escape", Unsafe::LinkOutside),
        ("malicious/symlink.7z", "escape", Unsafe::LinkUnchecked),
    ] {
        let outer = tempfile::tempdir().unwrap();
        let folder = outer.path().join("a").join("job");
        std::fs::create_dir_all(&folder).unwrap();
        let archive = Path::new(fixture).file_name().unwrap();
        std::fs::copy(fixtures().join(fixture), folder.join(archive)).unwrap();

        let err = extract(&folder, None, &delete()).unwrap_err();

        assert_eq!(
            err,
            Error::Unsafe {
                archive: Some(archive.to_string_lossy().into_owned()),
                entry: entry.to_owned(),
                why,
            },
            "{fixture}"
        );
        assert_eq!(
            names_in(outer.path()),
            [format!("a/job/{}", archive.to_string_lossy())],
            "{fixture}"
        );
    }
}

#[test]
fn safety_passes_clean_jobs_and_says_what_it_checked() {
    let mut files = RAR5_VOLUMES.to_vec();
    files.extend(["7z/show.7z"]);
    let dir = job(&files);

    let checked = inspect(dir.path(), None, &blocked()).unwrap();

    assert_eq!(
        checked.to_string(),
        "checked 4 files and 2 archives with 3 entries; nothing blocked"
    );
    assert_eq!(names_in(dir.path()).len(), 4);
}

#[test]
fn safety_blocks_executables_inside_archives() {
    for (fixture, archive, file) in [
        ("malicious/executable.rar", "executable.rar", "setup.exe"),
        ("malicious/shortcut.7z", "shortcut.7z", "Watch Now.lnk"),
    ] {
        let dir = job(&[fixture]);
        let err = inspect(dir.path(), None, &blocked()).unwrap_err();
        assert_eq!(
            err,
            Error::Blocked {
                archive: Some(archive.to_owned()),
                file: file.to_owned()
            }
        );
        assert_eq!(
            err.to_string(),
            format!("{archive} contains `{file}`, a blocked file type")
        );
    }
}

#[test]
fn safety_blocks_executables_lying_in_the_job_folder() {
    let dir = job(&["7z/show.7z"]);
    std::fs::create_dir(dir.path().join("Extras")).unwrap();
    std::fs::write(dir.path().join("Extras/Play.BAT"), b"@echo off").unwrap();

    let err = inspect(dir.path(), None, &blocked()).unwrap_err();

    assert_eq!(
        err.to_string(),
        "the job contains `Extras/Play.BAT`, a blocked file type"
    );
}

#[test]
fn safety_refuses_paths_and_links_leaving_the_job_folder() {
    for (fixture, why) in [
        ("malicious/zip-slip.rar", Unsafe::ParentDirectory),
        ("malicious/zip-slip.7z", Unsafe::ParentDirectory),
        ("malicious/absolute.rar", Unsafe::Absolute),
        ("malicious/drive.7z", Unsafe::Absolute),
        ("malicious/reserved-name.rar", Unsafe::InvalidName),
        ("malicious/symlink.rar", Unsafe::LinkOutside),
        ("malicious/symlink.7z", Unsafe::LinkUnchecked),
    ] {
        let dir = job(&[fixture]);
        match inspect(dir.path(), None, &blocked()) {
            Err(Error::Unsafe { why: found, .. }) => assert_eq!(found, why, "{fixture}"),
            other => panic!("{fixture}: {other:?}"),
        }
    }
}

#[test]
fn safety_needs_the_password_when_names_are_encrypted() {
    let dir = job(&["rar5-header-password/hidden.rar"]);
    assert!(matches!(
        inspect(dir.path(), None, &blocked()),
        Err(Error::PasswordMissing { .. })
    ));
    assert!(inspect(dir.path(), Some(PASSWORD), &blocked()).is_ok());

    // Names stay readable when only the data is encrypted, so no password is needed to check them.
    let dir = job(&["rar5-password/secret.rar"]);
    assert!(inspect(dir.path(), None, &blocked()).is_ok());
}

fn context(folder: &Path, password: Option<&str>) -> JobContext {
    JobContext {
        job_id: 1,
        name: "show".to_owned(),
        category: None,
        folder: folder.to_owned(),
        password: password.map(str::to_owned),
    }
}

#[tokio::test]
async fn safety_provider_fails_the_job_with_the_reason() {
    let dir = job(&["malicious/executable.rar"]);
    let check = SafetyCheck::new(SafetySettings {
        blocked_extensions: blocked(),
    });
    assert_eq!(check.info().id, "postproc-safety");

    let outcome = check.process(context(dir.path(), None)).await.unwrap();

    assert_eq!(
        outcome,
        PostProcessOutcome::Fail {
            reason: "executable.rar contains `setup.exe`, a blocked file type".to_owned()
        }
    );
}

#[tokio::test]
async fn extract_provider_uses_the_job_password() {
    let dir = job(&["7z-password/secret.7z"]);
    let extractor = Extractor::new(delete());
    assert_eq!(extractor.info().id, "postproc-extract");

    let outcome = extractor
        .process(context(dir.path(), Some(PASSWORD)))
        .await
        .unwrap();

    assert_eq!(
        outcome,
        PostProcessOutcome::Continue {
            message: "secret.7z (1 volume): extracted 1 file; deleted 1 archive file".to_owned()
        }
    );
    assert_eq!(names_in(dir.path()), ["secret.mkv"]);
}

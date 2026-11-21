use std::path::Path;

use fangst_postproc_par2::{Error, Volume, choose_volumes, repair, sets, verify};

const SLICE: usize = 4096;

fn data(len: usize, seed: u32) -> Vec<u8> {
    // No zero runs: zeroing a range must change every block it touches.
    (0..len)
        .map(|i| {
            let x = u32::try_from(i).unwrap().wrapping_mul(seed) ^ seed;
            (x ^ (x >> 9)).to_le_bytes()[0] | 1
        })
        .collect()
}

/// Writes the data files and the PAR2 files of a set with volumes of 1, 2, 4 and 8 blocks.
fn write_set(dir: &Path, files: &[(&str, &[u8])], volumes: bool) {
    for (name, contents) in files {
        std::fs::write(dir.join(name), contents).unwrap();
    }
    let created = fangst_bench::par2::create("set", files, SLICE, &[1, 2, 4, 8]);
    let keep = if volumes { created.len() } else { 1 };
    for (name, contents) in &created[..keep] {
        std::fs::write(dir.join(name), contents).unwrap();
    }
}

fn damage(path: &Path, from: usize, len: usize) {
    let mut contents = std::fs::read(path).unwrap();
    contents[from..from + len].fill(0);
    std::fs::write(path, contents).unwrap();
}

#[test]
fn groups_par2_file_names_into_sets() {
    let names = [
        "Show.S01E01.par2",
        "Show.S01E01.vol00+01.par2",
        "show.s01e01.vol01+02.PAR2",
        "Show.S01E01.vol003+004.par2",
        "show.s01e01.r00",
        "Other.par2",
    ];

    let found = sets(names);

    assert_eq!(found.len(), 2);
    let show = found
        .iter()
        .find(|s| s.index.as_deref() == Some("Show.S01E01.par2"))
        .unwrap();
    let volumes: Vec<(&str, u32, u32)> = show
        .volumes
        .iter()
        .map(|v| (v.name.as_str(), v.first, v.blocks))
        .collect();
    assert_eq!(
        volumes,
        [
            ("Show.S01E01.vol00+01.par2", 0, 1),
            ("show.s01e01.vol01+02.PAR2", 1, 2),
            ("Show.S01E01.vol003+004.par2", 3, 4),
        ]
    );
    let other = found
        .iter()
        .find(|s| s.index.as_deref() == Some("Other.par2"))
        .unwrap();
    assert!(other.volumes.is_empty());
}

#[test]
fn a_set_without_index_file_still_lists_its_volumes() {
    let found = sets(["lonely.vol0+5.par2"]);

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].index, None);
    assert_eq!(found[0].volumes[0].blocks, 5);
}

fn volumes(blocks: &[u32]) -> Vec<Volume> {
    let mut first = 0;
    blocks
        .iter()
        .map(|&b| {
            let v = Volume {
                name: format!("v{first}+{b}.par2"),
                first,
                blocks: b,
            };
            first += b;
            v
        })
        .collect()
}

#[test]
fn picks_the_fewest_recovery_blocks_that_cover_the_damage() {
    let vols = volumes(&[1, 2, 4, 8]);
    let total = |picked: &[usize]| picked.iter().map(|&i| vols[i].blocks).sum::<u32>();

    assert_eq!(choose_volumes(&vols, 0), Some(vec![]));
    assert_eq!(total(&choose_volumes(&vols, 3).unwrap()), 3);
    assert_eq!(total(&choose_volumes(&vols, 5).unwrap()), 5);
    assert_eq!(choose_volumes(&vols, 4), Some(vec![2]));
    assert_eq!(total(&choose_volumes(&vols, 15).unwrap()), 15);
    assert_eq!(choose_volumes(&vols, 16), None);
    // No exact fit: the smallest overshoot wins.
    assert_eq!(choose_volumes(&volumes(&[8, 8, 3]), 4), Some(vec![0]));
}

#[test]
fn intact_set_verifies_without_recovery_volumes() {
    let dir = tempfile::tempdir().unwrap();
    let a = data(50_000, 7);
    let b = data(9_000, 13);
    write_set(dir.path(), &[("a.bin", &a), ("b.bin", &b)], false);

    let result = verify(dir.path(), &dir.path().join("set.par2")).unwrap();

    assert!(result.is_intact(), "{result:?}");
    assert_eq!(result.blocks_needed, 0);
    let mut files = result.files.clone();
    files.sort();
    assert_eq!(files, ["a.bin", "b.bin"]);
}

#[test]
fn damaged_and_missing_files_count_the_blocks_needed() {
    let dir = tempfile::tempdir().unwrap();
    let a = data(50_000, 7);
    let b = data(9_000, 13);
    write_set(dir.path(), &[("a.bin", &a), ("b.bin", &b)], false);
    // Two bytes on each side of a slice boundary damage two slices.
    damage(&dir.path().join("a.bin"), SLICE * 3 - 2, 4);
    std::fs::remove_file(dir.path().join("b.bin")).unwrap();

    let result = verify(dir.path(), &dir.path().join("set.par2")).unwrap();

    assert!(!result.is_intact());
    assert_eq!(result.blocks_needed, 2 + 3);
    assert_eq!(result.damaged, [("a.bin".to_owned(), 2)]);
    assert_eq!(result.missing, [("b.bin".to_owned(), 3)]);
}

#[test]
fn repairs_damage_with_enough_recovery_blocks() {
    let dir = tempfile::tempdir().unwrap();
    let a = data(50_000, 7);
    let b = data(9_000, 13);
    write_set(dir.path(), &[("a.bin", &a), ("b.bin", &b)], true);
    damage(&dir.path().join("a.bin"), 100, 10_000);
    std::fs::remove_file(dir.path().join("b.bin")).unwrap();

    let repaired = repair(dir.path(), &dir.path().join("set.par2")).unwrap();

    assert_eq!(repaired.blocks, 3 + 3);
    assert_eq!(std::fs::read(dir.path().join("a.bin")).unwrap(), a);
    assert_eq!(std::fs::read(dir.path().join("b.bin")).unwrap(), b);
}

#[test]
fn too_few_recovery_blocks_is_an_error_that_says_how_many() {
    let dir = tempfile::tempdir().unwrap();
    let a = data(100_000, 7);
    write_set(dir.path(), &[("a.bin", &a)], true);
    damage(&dir.path().join("a.bin"), 0, 16 * SLICE);

    let err = repair(dir.path(), &dir.path().join("set.par2")).unwrap_err();

    assert_eq!(
        err,
        Error::NotEnoughRecovery {
            needed: 16,
            available: 15
        }
    );
    assert_eq!(
        err.to_string(),
        "PAR2 repair needs 16 recovery blocks, only 15 are available"
    );
}

#[test]
fn names_that_leave_the_folder_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let job = dir.path().join("job");
    std::fs::create_dir(&job).unwrap();
    let evil = data(5_000, 3);
    for (name, contents) in
        fangst_bench::par2::create("set", &[("../evil.bin", &evil)], SLICE, &[2])
    {
        std::fs::write(job.join(name), contents).unwrap();
    }

    let verify_err = verify(&job, &job.join("set.par2")).unwrap_err();
    let repair_err = repair(&job, &job.join("set.par2")).unwrap_err();

    assert_eq!(verify_err, Error::UnsafeName("../evil.bin".to_owned()));
    assert_eq!(repair_err, verify_err);
    assert!(!dir.path().join("evil.bin").exists());
}

#[test]
fn unreadable_par2_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("set.par2"), b"not a par2 file").unwrap();

    let err = verify(dir.path(), &dir.path().join("set.par2")).unwrap_err();

    assert!(matches!(err, Error::Unreadable(_)), "{err:?}");
}

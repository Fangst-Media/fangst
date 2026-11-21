use std::path::PathBuf;

use fangst_providers::NotificationEvent;

#[test]
fn completed_names_the_job_and_category() {
    let event = NotificationEvent::JobCompleted {
        job_id: 7,
        name: "Some.Show.S01E01".to_owned(),
        category: Some("tv".to_owned()),
    };
    assert_eq!(event.title(), "Download completed");
    assert_eq!(event.message(), "Some.Show.S01E01 (tv)");
}

#[test]
fn failed_gives_the_reason() {
    let event = NotificationEvent::JobFailed {
        job_id: 7,
        name: "Some.Show.S01E01".to_owned(),
        reason: "3 segments missing".to_owned(),
    };
    assert_eq!(event.title(), "Download failed");
    assert_eq!(event.message(), "Some.Show.S01E01: 3 segments missing");
}

#[test]
fn disk_full_gives_free_space_in_readable_units() {
    let event = NotificationEvent::DiskFull {
        path: PathBuf::from("downloads"),
        free_bytes: 5 * 1024 * 1024 * 1024 + 512 * 1024 * 1024,
    };
    assert_eq!(event.title(), "Disk almost full");
    assert_eq!(event.message(), "downloads has 5.5 GiB free");

    let small = NotificationEvent::DiskFull {
        path: PathBuf::from("downloads"),
        free_bytes: 300 * 1024 * 1024,
    };
    assert_eq!(small.message(), "downloads has 300 MiB free");
}

#[test]
fn event_names_are_stable() {
    let event = NotificationEvent::DiskFull {
        path: PathBuf::from("d"),
        free_bytes: 1,
    };
    assert_eq!(event.name(), "disk_full");
    let json = serde_json::to_value(&event).unwrap();
    assert_eq!(json["event"], "disk_full");
}

//! Categories: labels on jobs that clients can also create before any job uses them.

use fangst_downloads::{Error, Store, TorrentSource};

const MAGNET: &str = "magnet:?xt=urn:btih:cab507494d02ebb1178b38f2e9d7be299c86b862&dn=x";

#[tokio::test]
async fn categories_are_the_created_ones_and_those_jobs_use() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    assert_eq!(
        store.categories().await.unwrap(),
        [] as [std::string::String; 0]
    );

    store.create_category("tv-sonarr").await.unwrap();
    // Creating twice is not an error; clients create before every add.
    store.create_category("tv-sonarr").await.unwrap();
    let id = store
        .add_torrent("x", Some("movies"), 0, TorrentSource::Magnet(MAGNET))
        .await
        .unwrap();
    assert_eq!(store.categories().await.unwrap(), ["movies", "tv-sonarr"]);

    for bad in ["", "  ", "a\nb"] {
        assert!(
            matches!(store.create_category(bad).await, Err(Error::Invalid(_))),
            "{bad:?}"
        );
    }

    store.set_category(id, Some("tv-imported")).await.unwrap();
    let job = store.get(id).await.unwrap();
    assert_eq!(job.category.as_deref(), Some("tv-imported"));
    let timeline = store.timeline(id).await.unwrap();
    assert_eq!(timeline.last().unwrap().kind, "category");
    assert_eq!(
        store.categories().await.unwrap(),
        ["tv-imported", "tv-sonarr"],
        "a category only a job used goes with the job"
    );
    store.set_category(id, None).await.unwrap();
    assert_eq!(store.get(id).await.unwrap().category, None);
    assert!(matches!(
        store.set_category(99, Some("tv")).await,
        Err(Error::NotFound(99))
    ));
}

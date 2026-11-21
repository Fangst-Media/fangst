use std::sync::Arc;
use std::time::Duration;

use fangst_core::secret::MasterKey;
use fangst_downloads::{Auth, ENVIRONMENT_KEY, Error, FIRST_USER, Secrets, Store};

// Placeholders only; never real credentials.
const PASSWORD: &str = "placeholder-password";
const USENET_PASSWORD: &str = "usenet-placeholder-7f3a";

struct Setup {
    dir: tempfile::TempDir,
    store: Store,
    key: Arc<MasterKey>,
}

async fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(&dir.path().join("downloads.db")).await.unwrap();
    let key = Arc::new(MasterKey::load_or_create(&dir.path().join("master.key")).unwrap());
    Setup { dir, store, key }
}

fn database_bytes(dir: &std::path::Path) -> Vec<u8> {
    let mut all = Vec::new();
    for name in ["downloads.db", "downloads.db-wal"] {
        if let Ok(bytes) = std::fs::read(dir.join(name)) {
            all.extend(bytes);
        }
    }
    all
}

fn contains(haystack: &[u8], needle: &str) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w == needle.as_bytes())
}

#[tokio::test]
async fn auth_bootstrap_gives_the_first_user_the_environment_key() {
    let s = setup().await;
    let auth = Auth::new(&s.store, s.key.clone()).await.unwrap();
    let admin = auth.bootstrap("first-environment-key").await.unwrap();
    assert_eq!(admin.username, FIRST_USER);
    assert_eq!(
        auth.user_for_api_key("first-environment-key")
            .await
            .unwrap(),
        Some(admin.clone())
    );
    assert_eq!(
        auth.api_key(admin.id, ENVIRONMENT_KEY)
            .await
            .unwrap()
            .as_deref(),
        Some("first-environment-key")
    );

    let again = auth.bootstrap("rotated-environment-key").await.unwrap();
    assert_eq!(again, admin, "a restart reuses the first user");
    assert_eq!(
        auth.user_for_api_key("first-environment-key")
            .await
            .unwrap(),
        None,
        "rotating the variable revokes the old key"
    );
    assert_eq!(
        auth.user_for_api_key("rotated-environment-key")
            .await
            .unwrap(),
        Some(admin)
    );
}

#[tokio::test]
async fn auth_password_login() {
    let s = setup().await;
    let auth = Auth::new(&s.store, s.key.clone()).await.unwrap();
    let user = auth.create_user("someone", Some(PASSWORD)).await.unwrap();
    assert_eq!(
        auth.login("someone", PASSWORD).await.unwrap(),
        Some(user.clone())
    );
    assert_eq!(
        auth.login("SOMEONE", PASSWORD).await.unwrap(),
        Some(user.clone()),
        "usernames ignore case"
    );
    assert_eq!(
        auth.login("someone", "wrong-password-here").await.unwrap(),
        None
    );
    assert_eq!(auth.login("nobody", PASSWORD).await.unwrap(), None);

    let no_password = auth.create_user("keyonly", None).await.unwrap();
    assert!(!auth.has_password(no_password.id).await.unwrap());
    assert_eq!(auth.login("keyonly", "").await.unwrap(), None);

    assert!(matches!(
        auth.create_user("Someone", None).await,
        Err(Error::Invalid(_))
    ));
    assert!(matches!(
        auth.set_password(user.id, "short").await,
        Err(Error::Invalid(_))
    ));

    let hashes = database_bytes(s.dir.path());
    assert!(!contains(&hashes, PASSWORD), "passwords are stored hashed");
}

#[tokio::test]
async fn auth_sessions_expire_and_end() {
    let s = setup().await;
    let auth = Auth::new(&s.store, s.key.clone()).await.unwrap();
    let user = auth.create_user("someone", Some(PASSWORD)).await.unwrap();

    let token = auth
        .start_session(user.id, Duration::from_hours(1))
        .await
        .unwrap();
    assert_eq!(
        auth.user_for_session(&token).await.unwrap(),
        Some(user.clone())
    );
    assert!(
        !contains(&database_bytes(s.dir.path()), &token),
        "only a hash of the token is stored"
    );
    auth.end_session(&token).await.unwrap();
    assert_eq!(auth.user_for_session(&token).await.unwrap(), None);

    let expired = auth.start_session(user.id, Duration::ZERO).await.unwrap();
    assert_eq!(auth.user_for_session(&expired).await.unwrap(), None);

    let token = auth
        .start_session(user.id, Duration::from_hours(1))
        .await
        .unwrap();
    auth.set_password(user.id, "another-placeholder")
        .await
        .unwrap();
    assert_eq!(
        auth.user_for_session(&token).await.unwrap(),
        None,
        "changing the password ends open sessions"
    );
}

#[tokio::test]
async fn auth_stored_usenet_passwords_are_not_readable_in_the_database() {
    let s = setup().await;
    let secrets = Secrets::new(&s.store, s.key.clone());
    let auth = Auth::new(&s.store, s.key.clone()).await.unwrap();
    secrets
        .put("usenet:news.example.com", USENET_PASSWORD)
        .await
        .unwrap();
    auth.bootstrap("environment-key-placeholder").await.unwrap();
    // Push everything out of the WAL so the main file is what a copied database would hold.
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(
            &sqlx::SqlitePool::connect(&format!(
                "sqlite://{}",
                s.dir.path().join("downloads.db").display()
            ))
            .await
            .unwrap(),
        )
        .await
        .unwrap();

    let bytes = database_bytes(s.dir.path());
    assert!(!contains(&bytes, USENET_PASSWORD));
    assert!(!contains(&bytes, "environment-key-placeholder"));

    assert_eq!(
        secrets
            .get("usenet:news.example.com")
            .await
            .unwrap()
            .as_deref(),
        Some(USENET_PASSWORD)
    );
    assert_eq!(secrets.get("usenet:missing").await.unwrap(), None);

    let other_key = Arc::new(MasterKey::from_bytes(&[1; 32]));
    let wrong = Secrets::new(&s.store, other_key);
    assert!(matches!(
        wrong.get("usenet:news.example.com").await,
        Err(Error::Secret(_))
    ));

    secrets.delete("usenet:news.example.com").await.unwrap();
    assert_eq!(secrets.get("usenet:news.example.com").await.unwrap(), None);
}

#[tokio::test]
async fn auth_changed_environment_key_ends_the_first_users_sessions() {
    let s = setup().await;
    let auth = Auth::new(&s.store, s.key.clone()).await.unwrap();
    let admin = auth.bootstrap("first-environment-key").await.unwrap();
    auth.set_password(admin.id, PASSWORD).await.unwrap();
    let session = auth
        .start_session(admin.id, Duration::from_hours(1))
        .await
        .unwrap();

    auth.bootstrap("first-environment-key").await.unwrap();
    assert_eq!(
        auth.user_for_session(&session).await.unwrap(),
        Some(admin.clone()),
        "a restart with the same key keeps people signed in"
    );

    auth.bootstrap("rotated-environment-key").await.unwrap();
    assert_eq!(
        auth.user_for_session(&session).await.unwrap(),
        None,
        "whoever held the old key may have opened that session"
    );
}

#[tokio::test]
async fn auth_reset_finds_users_by_name_and_signs_them_out() {
    let s = setup().await;
    let auth = Auth::new(&s.store, s.key.clone()).await.unwrap();
    let user = auth.create_user("someone", Some(PASSWORD)).await.unwrap();
    let session = auth
        .start_session(user.id, Duration::from_hours(1))
        .await
        .unwrap();
    assert_eq!(
        auth.user_by_name("SOMEONE").await.unwrap(),
        Some(user.clone())
    );
    assert_eq!(auth.user_by_name("nobody").await.unwrap(), None);
    auth.end_sessions(user.id).await.unwrap();
    assert_eq!(auth.user_for_session(&session).await.unwrap(), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auth_password_checks_past_the_limit_are_refused_not_queued() {
    let s = setup().await;
    let auth = Auth::with_hash_slots(&s.store, s.key.clone(), 1)
        .await
        .unwrap();
    auth.create_user("someone", Some(PASSWORD)).await.unwrap();

    let attempts: Vec<_> = (0..8)
        .map(|_| {
            let auth = auth.clone();
            tokio::spawn(async move { auth.login("someone", PASSWORD).await })
        })
        .collect();
    let mut ok = 0;
    let mut busy = 0;
    for attempt in attempts {
        match attempt.await.unwrap() {
            Ok(Some(_)) => ok += 1,
            Err(Error::Busy) => busy += 1,
            other => panic!("unexpected {other:?}"),
        }
    }
    assert!(ok >= 1, "one slot means at least one check runs");
    assert!(busy >= 1, "the rest are refused while it runs");
    assert_eq!(ok + busy, 8);

    assert!(
        auth.login("someone", PASSWORD).await.unwrap().is_some(),
        "slots come back once checks finish"
    );
}

#[tokio::test]
async fn only_the_first_user_is_the_owner() {
    let s = setup().await;
    let auth = Auth::new(&s.store, s.key.clone()).await.unwrap();
    assert_eq!(auth.owner().await.unwrap(), None);
    let admin = auth.bootstrap("first-environment-key").await.unwrap();
    let other = auth.create_user("someone", None).await.unwrap();
    assert_eq!(auth.owner().await.unwrap(), Some(admin.clone()));
    assert!(auth.is_owner(admin.id).await.unwrap());
    assert!(!auth.is_owner(other.id).await.unwrap());
}

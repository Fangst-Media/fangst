//! Checks the parser against `fixtures/release-names/`, which were produced by running the old
//! TypeScript parser (`fixtures/release-names/export.ts`), so every field must match exactly.

use fangst_release::{
    ParsedRelease, TierSource, detect_streaming_service, parse, release_group_tier,
};
use serde_json::{Map, Value, json};

fn fixture(name: &str) -> Vec<Value> {
    let path = manifest_dir()
        .join("../../fixtures/release-names")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

/// The shape `JSON.stringify` gave the TypeScript result: absent fields are left out, and the
/// tier is present (possibly `null`) exactly when a group was found.
fn ts_shape(r: &ParsedRelease) -> Value {
    let mut m = Map::new();
    m.insert("title".into(), json!(r.title));
    if let Some(v) = r.resolution {
        m.insert("resolution".into(), json!(v.as_str()));
    }
    if let Some(v) = r.source {
        m.insert("source".into(), json!(v.as_str()));
    }
    m.insert("isRemux".into(), json!(r.is_remux));
    if let Some(v) = &r.streaming_service {
        m.insert("streamingService".into(), json!(v));
    }
    if let Some(v) = r.video_codec {
        m.insert("videoCodec".into(), json!(v.as_str()));
    }
    if let Some(v) = r.audio_codec {
        m.insert("audioCodec".into(), json!(v.as_str()));
    }
    if let Some(v) = r.hdr {
        m.insert("hdr".into(), json!(v.as_str()));
    }
    if let Some(v) = r.year {
        m.insert("year".into(), json!(v));
    }
    if let Some(v) = r.season {
        m.insert("season".into(), json!(v));
    }
    if let Some(v) = r.episode {
        m.insert("episode".into(), json!(v));
    }
    if let Some(v) = &r.release_group {
        m.insert("releaseGroup".into(), json!(v));
        m.insert(
            "releaseGroupTier".into(),
            json!(r.release_group_tier.map(u8::from)),
        );
    }
    m.insert("proper".into(), json!(r.proper));
    m.insert("repack".into(), json!(r.repack));
    m.insert("qualityScore".into(), json!(r.quality_score));
    Value::Object(m)
}

#[test]
fn every_parse_fixture_matches() {
    let cases = fixture("parse.json");
    assert!(cases.len() >= 20, "fixtures missing");
    let mut failures = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let got = ts_shape(&parse(name));
        if got != case["expected"] {
            failures.push(format!(
                "{name:?}\n  want {}\n  got  {got}",
                case["expected"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

#[test]
fn every_streaming_fixture_matches() {
    for case in fixture("streaming-services.json") {
        let name = case["name"].as_str().unwrap();
        let got = detect_streaming_service(name).map(|s| s.abbreviation);
        assert_eq!(got, case["expected"].as_str(), "{name}");
    }
}

#[test]
fn every_group_tier_fixture_matches() {
    for case in fixture("group-tiers.json") {
        let group = case["group"].as_str().unwrap();
        let resolution = case["resolution"].as_str().unwrap();
        let source = match case["source"].as_str().unwrap() {
            "any" => TierSource::Any,
            "web_dl" => TierSource::WebDl,
            "bluray" => TierSource::Bluray,
            "dvd" => TierSource::Dvd,
            other => panic!("unknown source {other}"),
        };
        let got = release_group_tier(group, Some(resolution), source).map(u8::from);
        let want = case["expected"].as_u64().map(|t| u8::try_from(t).unwrap());
        assert_eq!(got, want, "{group} {resolution} {source:?}");
    }
}

/// The crate folder at run time. `env!("CARGO_MANIFEST_DIR")` is baked in at compile time, and a
/// build folder shared between git worktrees can hand back a test binary built in another (perhaps
/// deleted) worktree; cargo and nextest set the variable correctly for every run.
fn manifest_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("run through cargo or nextest"),
    )
}

use fangst_release::{TierSource, detect_streaming_service, parse, release_group_tier};
use proptest::prelude::*;

proptest! {
    #[test]
    fn arbitrary_unicode_never_panics(name in any::<String>()) {
        let parsed = parse(&name);
        prop_assert!(!parsed.title.starts_with(' ') && !parsed.title.ends_with(' '));
    }

    // Biased towards release-name tokens so the matching paths, not just the misses, are exercised.
    #[test]
    fn token_soup_never_panics(
        parts in prop::collection::vec(
            prop_oneof![
                Just("S01E02".to_owned()), Just("2160p".to_owned()), Just("WEB-DL".to_owned()),
                Just("DTS-HD MA".to_owned()), Just("Atmos".to_owned()), Just("HDR10+".to_owned()),
                Just("-GROUP[x]".to_owned()), Just("MA".to_owned()), Just("HBO".to_owned()),
                Just("MAX".to_owned()), Just("2001".to_owned()), Just("\u{e9}".to_owned()),
                "[a-zA-Z0-9]{0,6}",
                any::<String>(),
            ],
            0..12,
        ),
        sep in prop_oneof![Just("."), Just("_"), Just(" "), Just("-"), Just("")],
    ) {
        let name = parts.join(sep);
        let _ = parse(&name);
        let _ = detect_streaming_service(&name);
    }

    #[test]
    fn tier_lookup_never_panics(group in any::<String>(), resolution in any::<Option<String>>()) {
        for source in [TierSource::Any, TierSource::WebDl, TierSource::Bluray, TierSource::Dvd] {
            let _ = release_group_tier(&group, resolution.as_deref(), source);
        }
    }
}

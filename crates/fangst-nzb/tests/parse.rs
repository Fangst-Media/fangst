use fangst_nzb::{Error, Limits, parse};
use proptest::prelude::*;

const SAMPLE: &[u8] = include_bytes!("../../../fixtures/nzb/password-two-files.nzb");

#[test]
fn parses_meta_with_entities_decoded() {
    let nzb = parse(SAMPLE, &Limits::default()).unwrap();
    assert_eq!(nzb.title(), Some("Sample Release"));
    assert_eq!(nzb.password(), Some("s3cr&t"));
    assert_eq!(nzb.category(), Some("TV > HD"));
}

#[test]
fn parses_files_groups_and_segments_in_number_order() {
    let nzb = parse(SAMPLE, &Limits::default()).unwrap();
    assert_eq!(nzb.files.len(), 2);
    let rar = &nzb.files[0];
    assert_eq!(rar.groups, ["alt.binaries.test", "alt.binaries.misc"]);
    assert_eq!(rar.date, Some(1_700_000_000));
    let numbers: Vec<u32> = rar.segments.iter().map(|s| s.number).collect();
    assert_eq!(numbers, [1, 2]);
    assert_eq!(rar.total_bytes(), 1_536_000);
}

#[test]
fn message_ids_lose_their_angle_brackets() {
    let nzb = parse(SAMPLE, &Limits::default()).unwrap();
    assert_eq!(
        nzb.files[0].segments[0].message_id,
        "part1of2.abc@example.invalid"
    );
}

#[test]
fn file_name_comes_from_the_quoted_part_of_the_subject() {
    let nzb = parse(SAMPLE, &Limits::default()).unwrap();
    assert_eq!(
        nzb.files[0].file_name().as_deref(),
        Some("sample.part01.rar")
    );
    assert!(nzb.files[1].is_par2());
    assert!(!nzb.files[0].is_par2());
}

#[test]
fn file_name_falls_back_to_the_token_before_yenc() {
    let nzb = minimal("Some.Show.S01E01.mkv yEnc (1/1)");
    assert_eq!(
        nzb.files[0].file_name().as_deref(),
        Some("Some.Show.S01E01.mkv")
    );
}

#[test]
fn duplicate_segment_numbers_keep_the_first() {
    let xml = br#"<nzb><file subject="a"><groups><group>g</group></groups><segments>
        <segment bytes="1" number="1">first@x</segment>
        <segment bytes="1" number="1">second@x</segment>
    </segments></file></nzb>"#;
    let nzb = parse(xml, &Limits::default()).unwrap();
    assert_eq!(nzb.files[0].segments.len(), 1);
    assert_eq!(nzb.files[0].segments[0].message_id, "first@x");
}

#[test]
fn missing_segment_numbers_are_reported() {
    let xml = br#"<nzb><file subject="a"><groups><group>g</group></groups><segments>
        <segment bytes="1" number="1">a@x</segment>
        <segment bytes="1" number="4">b@x</segment>
    </segments></file></nzb>"#;
    let nzb = parse(xml, &Limits::default()).unwrap();
    assert_eq!(nzb.files[0].missing_segments(), [2, 3]);
}

#[test]
fn oversized_input_is_refused_before_parsing() {
    let limits = Limits {
        max_bytes: 100,
        ..Limits::default()
    };
    assert!(matches!(
        parse(SAMPLE, &limits),
        Err(Error::TooLarge { .. })
    ));
}

#[test]
fn too_many_segments_is_refused() {
    let limits = Limits {
        max_segments: 2,
        ..Limits::default()
    };
    assert!(matches!(
        parse(SAMPLE, &limits),
        Err(Error::TooManySegments { .. })
    ));
}

#[test]
fn empty_nzb_is_an_error() {
    assert!(matches!(
        parse(b"<nzb></nzb>", &Limits::default()),
        Err(Error::NoFiles)
    ));
}

#[test]
fn not_an_nzb_is_an_error() {
    assert!(matches!(
        parse(b"<html><body/></html>", &Limits::default()),
        Err(Error::NotAnNzb)
    ));
}

#[test]
fn broken_xml_is_an_error() {
    assert!(matches!(
        parse(b"<nzb><file subject=\"a\"><segments>", &Limits::default()),
        Err(Error::Xml(_))
    ));
}

#[test]
fn segment_without_message_id_is_an_error() {
    let xml = br#"<nzb><file subject="a"><groups><group>g</group></groups><segments>
        <segment bytes="1" number="1">  </segment></segments></file></nzb>"#;
    assert!(matches!(
        parse(xml, &Limits::default()),
        Err(Error::BadSegment { .. })
    ));
}

#[test]
fn segment_number_zero_is_an_error() {
    let xml = br#"<nzb><file subject="a"><groups><group>g</group></groups><segments>
        <segment bytes="1" number="0">a@x</segment></segments></file></nzb>"#;
    assert!(matches!(
        parse(xml, &Limits::default()),
        Err(Error::BadSegment { .. })
    ));
}

#[test]
fn entity_declarations_are_not_expanded() {
    // A billion-laughs style document must not blow up memory or succeed with expanded text.
    let xml = br#"<?xml version="1.0"?>
<!DOCTYPE nzb [<!ENTITY a "aaaaaaaaaa"><!ENTITY b "&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;">]>
<nzb><head><meta type="title">&b;</meta></head><file subject="a"><groups><group>g</group></groups>
<segments><segment bytes="1" number="1">a@x</segment></segments></file></nzb>"#;
    let result = parse(xml, &Limits::default());
    assert!(result.is_err() || result.unwrap().title() != Some(&"a".repeat(100)[..]));
}

fn minimal(subject: &str) -> fangst_nzb::Nzb {
    let xml = format!(
        r#"<nzb><file subject="{subject}"><groups><group>g</group></groups><segments><segment bytes="1" number="1">a@x</segment></segments></file></nzb>"#
    );
    parse(xml.as_bytes(), &Limits::default()).unwrap()
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(input in proptest::collection::vec(any::<u8>(), 0..4096)) {
        let _ = parse(&input, &Limits::default());
    }

    #[test]
    fn mutated_sample_never_panics(cut in 0usize..SAMPLE.len(), byte in any::<u8>()) {
        let mut input = SAMPLE.to_vec();
        input[cut] = byte;
        let _ = parse(&input, &Limits::default());
        let _ = parse(&SAMPLE[..cut], &Limits::default());
    }
}

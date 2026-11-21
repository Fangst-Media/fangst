use fangst_yenc::{Error, PartInfo, crc32, decode, encode_part, encode_single};
use proptest::prelude::*;

#[test]
fn crc32_matches_the_standard_check_value() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
}

#[test]
fn decodes_a_hand_written_single_part_article() {
    // "Hi" is 0x48 0x69; +42 gives 0x72 'r' and 0x93.
    let article =
        b"=ybegin line=128 size=2 name=hi.txt\r\nr\x93\r\n=yend size=2 crc32=4d170e0e\r\n";
    let decoded = decode(article).unwrap();
    assert_eq!(decoded.data, b"Hi");
    assert_eq!(decoded.header.name, "hi.txt");
    assert_eq!(decoded.header.size, 2);
    assert_eq!(decoded.part, None);
}

#[test]
fn file_names_keep_their_spaces() {
    let article = encode_single(b"x", "my file (1).mkv", 128);
    assert_eq!(decode(&article).unwrap().header.name, "my file (1).mkv");
}

#[test]
fn escaped_bytes_decode() {
    // NUL, LF, CR and '=' must be escaped by an encoder; this checks the decoder undoes it.
    let data = [0u8, b'\n', b'\r', b'=', 214, 224, 227, 19];
    let article = encode_single(&data, "esc.bin", 128);
    assert_eq!(decode(&article).unwrap().data, data);
}

#[test]
fn text_before_the_header_is_skipped() {
    let mut article = b"some preamble the poster added\r\n\r\n".to_vec();
    article.extend(encode_single(b"payload", "p.bin", 128));
    assert_eq!(decode(&article).unwrap().data, b"payload");
}

#[test]
fn multi_part_article_reports_its_range() {
    let file: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
    let article = encode_part(&file, 401, 800, 2, 3, "f.bin", 128);
    let decoded = decode(&article).unwrap();
    assert_eq!(decoded.data, &file[400..800]);
    assert_eq!(
        decoded.part,
        Some(PartInfo {
            number: 2,
            total: Some(3),
            begin: 401,
            end: 800
        })
    );
    assert_eq!(decoded.header.size, 1000);
}

#[test]
fn corrupted_data_fails_the_crc_check() {
    let mut article = encode_single(b"important bytes", "c.bin", 128);
    let pos = article.iter().position(|&b| b == b'\n').unwrap() + 1;
    article[pos] = article[pos].wrapping_add(1);
    assert!(matches!(decode(&article), Err(Error::CrcMismatch { .. })));
}

#[test]
fn truncated_article_fails_on_size() {
    let article = encode_single(b"0123456789", "t.bin", 4);
    let text = String::from_utf8_lossy(&article).replace("=yend size=10", "=yend size=11");
    assert!(matches!(
        decode(text.as_bytes()),
        Err(Error::SizeMismatch {
            expected: 11,
            actual: 10
        })
    ));
}

#[test]
fn missing_header_is_an_error() {
    assert!(matches!(
        decode(b"no yenc here\r\n"),
        Err(Error::MissingHeader)
    ));
}

#[test]
fn missing_trailer_is_an_error() {
    assert!(matches!(
        decode(b"=ybegin line=128 size=1 name=a\r\nk\r\n"),
        Err(Error::MissingTrailer)
    ));
}

#[test]
fn header_without_size_is_an_error() {
    let article = b"=ybegin line=128 name=a\r\nk\r\n=yend size=1\r\n";
    assert!(matches!(decode(article), Err(Error::BadHeader { .. })));
}

#[test]
fn part_header_without_ypart_line_is_an_error() {
    let article = b"=ybegin part=1 line=128 size=1 name=a\r\nk\r\n=yend size=1 part=1\r\n";
    assert!(matches!(decode(article), Err(Error::BadHeader { .. })));
}

#[test]
fn part_length_must_match_its_range() {
    let file = vec![7u8; 100];
    let article = encode_part(&file, 1, 50, 1, 2, "r.bin", 128);
    let text = String::from_utf8_lossy(&article).replace("end=50", "end=60");
    assert!(matches!(
        decode(text.as_bytes()),
        Err(Error::PartRangeMismatch { .. })
    ));
}

proptest! {
    #[test]
    fn roundtrip_single(data in proptest::collection::vec(any::<u8>(), 0..4096), line in 1usize..300) {
        let decoded = decode(&encode_single(&data, "prop.bin", line)).unwrap();
        prop_assert_eq!(decoded.data, data);
    }

    #[test]
    fn roundtrip_parts(data in proptest::collection::vec(any::<u8>(), 1..4096), split in 0.0f64..1.0) {
        let len = data.len() as u64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
        let cut = ((len as f64 * split) as u64).clamp(1, len);
        let first = decode(&encode_part(&data, 1, cut, 1, 2, "p", 128)).unwrap();
        let mut joined = first.data;
        if cut < len {
            joined.extend(decode(&encode_part(&data, cut + 1, len, 2, 2, "p", 128)).unwrap().data);
        }
        prop_assert_eq!(joined, data);
    }

    #[test]
    fn arbitrary_input_never_panics(input in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = decode(&input);
    }

    #[test]
    fn arbitrary_text_after_header_never_panics(tail in "[ -~\r\n=]{0,512}") {
        let mut input = b"=ybegin part=1 total=2 line=128 size=10 name=x\r\n".to_vec();
        input.extend(tail.as_bytes());
        let _ = decode(&input);
    }
}

//! yEnc decoding and encoding.
//!
//! Decoding validates what the article claims about itself: the part range against the decoded
//! length, the trailer size, and the CRC32 (`pcrc32` for a part, `crc32` for a single-part file).
//! NNTP dot-unstuffing is the NNTP client's job and is assumed done.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    /// Size of the whole file, not of this part.
    pub size: u64,
    pub line: Option<u32>,
}

/// Where a part sits in the whole file. `begin` and `end` are 1-based and inclusive, as in `=ypart`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartInfo {
    pub number: u32,
    pub total: Option<u32>,
    pub begin: u64,
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub header: Header,
    pub part: Option<PartInfo>,
    pub data: Vec<u8>,
    /// CRC32 of `data`, already checked against the trailer when the trailer had one.
    pub crc32: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    MissingHeader,
    MissingTrailer,
    BadHeader { line: &'static str, reason: String },
    SizeMismatch { expected: u64, actual: u64 },
    PartRangeMismatch { begin: u64, end: u64, actual: u64 },
    CrcMismatch { expected: u32, actual: u32 },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHeader => f.write_str("no =ybegin line found"),
            Self::MissingTrailer => f.write_str("no =yend line found; the article is truncated"),
            Self::BadHeader { line, reason } => write!(f, "bad {line} line: {reason}"),
            Self::SizeMismatch { expected, actual } => {
                write!(f, "=yend says {expected} bytes but {actual} were decoded")
            }
            Self::PartRangeMismatch { begin, end, actual } => {
                write!(
                    f,
                    "=ypart covers bytes {begin}..={end} but {actual} were decoded"
                )
            }
            Self::CrcMismatch { expected, actual } => {
                write!(
                    f,
                    "CRC32 mismatch: article says {expected:08x}, data is {actual:08x}"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

pub fn decode(input: &[u8]) -> Result<Decoded, Error> {
    let mut lines = input.split(|&b| b == b'\n').map(trim_cr);

    let header_line = lines
        .by_ref()
        .find(|l| l.starts_with(b"=ybegin "))
        .ok_or(Error::MissingHeader)?;
    let begin = parse_keywords(&header_line[b"=ybegin ".len()..], true);
    let size = begin
        .number("size")
        .ok_or_else(|| bad("=ybegin", "missing size"))??;
    let name = begin
        .name
        .clone()
        .ok_or_else(|| bad("=ybegin", "missing name"))?;
    let line = begin
        .number("line")
        .transpose()?
        .and_then(|n| u32::try_from(n).ok());
    let part_number = begin.number("part").transpose()?;
    let total = begin
        .number("total")
        .transpose()?
        .and_then(|n| u32::try_from(n).ok());

    let mut part = None;
    let mut data = Vec::with_capacity(usize::try_from(size.min(1 << 24)).unwrap_or(0));
    let mut trailer = None;
    let mut first_body_line = true;

    for raw in lines {
        if first_body_line && part_number.is_some() {
            first_body_line = false;
            part = Some(parse_part_line(raw, part_number, total, size)?);
            continue;
        }
        first_body_line = false;
        if raw.starts_with(b"=yend") {
            trailer = Some(parse_keywords(
                raw.get(b"=yend ".len()..).unwrap_or_default(),
                false,
            ));
            break;
        }
        decode_line(raw, &mut data);
    }

    let trailer = trailer.ok_or(Error::MissingTrailer)?;
    let actual = data.len() as u64;

    if let Some(p) = part {
        let expected = p.end - p.begin + 1;
        if expected != actual {
            return Err(Error::PartRangeMismatch {
                begin: p.begin,
                end: p.end,
                actual,
            });
        }
    }
    if let Some(expected) = trailer.number("size").transpose()?
        && expected != actual
    {
        return Err(Error::SizeMismatch { expected, actual });
    }
    if part.is_none() && size != actual {
        return Err(Error::SizeMismatch {
            expected: size,
            actual,
        });
    }

    let crc = crc32(&data);
    let expected_crc = if part.is_some() {
        trailer.hex("pcrc32")
    } else {
        trailer.hex("crc32")
    };
    if let Some(expected) = expected_crc.transpose()?
        && expected != crc
    {
        return Err(Error::CrcMismatch {
            expected,
            actual: crc,
        });
    }

    Ok(Decoded {
        header: Header { name, size, line },
        part,
        data,
        crc32: crc,
    })
}

fn parse_part_line(
    raw: &[u8],
    number: Option<u64>,
    total: Option<u32>,
    size: u64,
) -> Result<PartInfo, Error> {
    let kw = raw
        .strip_prefix(b"=ypart ")
        .map(|rest| parse_keywords(rest, false))
        .ok_or_else(|| bad("=ypart", "=ybegin has part= but no =ypart line follows"))?;
    let begin = kw
        .number("begin")
        .ok_or_else(|| bad("=ypart", "missing begin"))??;
    let end = kw
        .number("end")
        .ok_or_else(|| bad("=ypart", "missing end"))??;
    if begin == 0 || end < begin || end > size {
        return Err(bad(
            "=ypart",
            format!("range {begin}..={end} does not fit a {size} byte file"),
        ));
    }
    Ok(PartInfo {
        number: number.and_then(|n| u32::try_from(n).ok()).unwrap_or(0),
        total,
        begin,
        end,
    })
}

fn decode_line(line: &[u8], out: &mut Vec<u8>) {
    let mut bytes = line.iter();
    while let Some(&b) = bytes.next() {
        if b == b'=' {
            // A trailing '=' with nothing after it is a broken encoder; dropping it keeps the rest usable
            // and the CRC check reports the damage.
            if let Some(&escaped) = bytes.next() {
                out.push(escaped.wrapping_sub(64).wrapping_sub(42));
            }
        } else {
            out.push(b.wrapping_sub(42));
        }
    }
}

fn trim_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn bad(line: &'static str, reason: impl Into<String>) -> Error {
    Error::BadHeader {
        line,
        reason: reason.into(),
    }
}

struct Keywords<'a> {
    pairs: Vec<(&'a [u8], &'a [u8])>,
    name: Option<String>,
}

impl Keywords<'_> {
    fn get(&self, key: &str) -> Option<&[u8]> {
        self.pairs
            .iter()
            .find(|(k, _)| *k == key.as_bytes())
            .map(|(_, v)| *v)
    }

    fn number(&self, key: &'static str) -> Option<Result<u64, Error>> {
        self.get(key).map(|v| {
            std::str::from_utf8(v)
                .ok()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| bad("yEnc", format!("{key}= is not a number")))
        })
    }

    fn hex(&self, key: &'static str) -> Option<Result<u32, Error>> {
        self.get(key).map(|v| {
            std::str::from_utf8(v)
                .ok()
                .and_then(|s| u32::from_str_radix(s.trim(), 16).ok())
                .ok_or_else(|| bad("=yend", format!("{key}= is not hex")))
        })
    }
}

/// Parses `key=value` pairs. In `=ybegin`, `name=` runs to the end of the line because names may contain spaces.
fn parse_keywords(text: &[u8], name_to_end: bool) -> Keywords<'_> {
    let mut pairs = Vec::new();
    let mut name = None;
    let mut rest = text;
    loop {
        rest = trim_start(rest);
        if rest.is_empty() {
            break;
        }
        if name_to_end && rest.starts_with(b"name=") {
            name = Some(
                String::from_utf8_lossy(&rest[b"name=".len()..])
                    .trim_end()
                    .to_owned(),
            );
            break;
        }
        let token_end = rest.iter().position(|&b| b == b' ').unwrap_or(rest.len());
        let token = &rest[..token_end];
        if let Some(eq) = token.iter().position(|&b| b == b'=') {
            pairs.push((&token[..eq], &token[eq + 1..]));
        }
        rest = &rest[token_end..];
    }
    Keywords { pairs, name }
}

fn trim_start(s: &[u8]) -> &[u8] {
    let start = s
        .iter()
        .position(|&b| b != b' ' && b != b'\t')
        .unwrap_or(s.len());
    &s[start..]
}

/// Encodes a whole file as one single-part article.
#[must_use]
pub fn encode_single(data: &[u8], name: &str, line_len: usize) -> Vec<u8> {
    let mut out = format!(
        "=ybegin line={line_len} size={} name={name}\r\n",
        data.len()
    )
    .into_bytes();
    encode_body(data, line_len, &mut out);
    out.extend(format!("=yend size={} crc32={:08x}\r\n", data.len(), crc32(data)).into_bytes());
    out
}

/// Encodes bytes `begin..=end` (1-based) of `file` as part `part` of `total`.
///
/// # Panics
/// When the range doesn't fit inside `file`.
#[must_use]
pub fn encode_part(
    file: &[u8],
    begin: u64,
    end: u64,
    part: u32,
    total: u32,
    name: &str,
    line_len: usize,
) -> Vec<u8> {
    let start = usize::try_from(begin - 1).expect("range fits in memory");
    let stop = usize::try_from(end).expect("range fits in memory");
    let chunk = &file[start..stop];
    let mut out = format!(
        "=ybegin part={part} total={total} line={line_len} size={} name={name}\r\n=ypart begin={begin} end={end}\r\n",
        file.len()
    )
    .into_bytes();
    encode_body(chunk, line_len, &mut out);
    out.extend(
        format!(
            "=yend size={} part={part} pcrc32={:08x} crc32={:08x}\r\n",
            chunk.len(),
            crc32(chunk),
            crc32(file)
        )
        .into_bytes(),
    );
    out
}

fn encode_body(data: &[u8], line_len: usize, out: &mut Vec<u8>) {
    let line_len = line_len.max(1);
    let mut col = 0;
    for (i, &byte) in data.iter().enumerate() {
        let enc = byte.wrapping_add(42);
        let at_edge = col == 0 || col + 1 >= line_len || i + 1 == data.len();
        // NUL, LF, CR and '=' always break the format; tab, space and '.' only at line edges,
        // where transports trim whitespace or NNTP treats a leading dot as special.
        let critical = matches!(enc, 0 | b'\n' | b'\r' | b'=')
            || (at_edge && matches!(enc, b'\t' | b' ' | b'.'));
        if critical {
            out.push(b'=');
            out.push(enc.wrapping_add(64));
            col += 2;
        } else {
            out.push(enc);
            col += 1;
        }
        if col >= line_len {
            out.extend_from_slice(b"\r\n");
            col = 0;
        }
    }
    if col > 0 {
        out.extend_from_slice(b"\r\n");
    }
}

/// CRC-32/ISO-HDLC, the variant yEnc uses.
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            #[allow(clippy::cast_possible_truncation)]
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 == 1 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            table[i] = c;
            i += 1;
        }
        table
    };
    !data.iter().fold(!0u32, |crc, &b| {
        TABLE[((crc ^ u32::from(b)) & 0xFF) as usize] ^ (crc >> 8)
    })
}

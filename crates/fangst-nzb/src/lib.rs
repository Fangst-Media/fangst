//! NZB parsing.
//!
//! NZB files come from indexers and users, so the parser treats them as hostile: input size, file
//! count and segment count are capped, and only the five predefined XML entities and character
//! references are resolved. Custom `<!ENTITY>` declarations are never expanded.

use std::fmt;

use quick_xml::Reader;
use quick_xml::events::{BytesRef, BytesStart, Event};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_files: usize,
    pub max_segments: usize,
}

impl Default for Limits {
    fn default() -> Self {
        // A 100 GB release in 700 KB segments is about 150k segments and an NZB of roughly 25 MB.
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_files: 100_000,
            max_segments: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nzb {
    /// `<meta type="...">` entries in document order.
    pub meta: Vec<(String, String)>,
    pub files: Vec<NzbFile>,
}

impl Nzb {
    #[must_use]
    pub fn meta(&self, kind: &str) -> Option<&str> {
        self.meta
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(kind))
            .map(|(_, v)| v.as_str())
    }

    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.meta("title")
    }

    #[must_use]
    pub fn password(&self) -> Option<&str> {
        self.meta("password")
    }

    #[must_use]
    pub fn category(&self) -> Option<&str> {
        self.meta("category")
    }

    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(NzbFile::total_bytes).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NzbFile {
    pub subject: String,
    pub poster: Option<String>,
    /// Unix timestamp of the post.
    pub date: Option<i64>,
    pub groups: Vec<String>,
    /// Sorted by number, one entry per number.
    pub segments: Vec<Segment>,
}

impl NzbFile {
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.segments.iter().map(|s| s.bytes).sum()
    }

    /// The file name, taken from the quoted part of the subject, or else the token before `yEnc`.
    #[must_use]
    pub fn file_name(&self) -> Option<String> {
        let subject = &self.subject;
        if let Some(start) = subject.find('"')
            && let Some(len) = subject[start + 1..].find('"')
        {
            let name = subject[start + 1..start + 1 + len].trim();
            if !name.is_empty() {
                return Some(name.to_owned());
            }
        }
        let before_yenc = subject.split(" yEnc").next().unwrap_or(subject);
        before_yenc
            .split_whitespace()
            .last()
            .filter(|t| t.contains('.'))
            .map(str::to_owned)
    }

    #[must_use]
    pub fn is_par2(&self) -> bool {
        self.file_name()
            .is_some_and(|n| n.to_ascii_lowercase().ends_with(".par2"))
    }

    /// Segment numbers between 1 and the highest number that the NZB doesn't list.
    #[must_use]
    pub fn missing_segments(&self) -> Vec<u32> {
        let mut missing = Vec::new();
        let mut expected = 1;
        for segment in &self.segments {
            missing.extend(expected..segment.number);
            expected = segment.number + 1;
        }
        missing
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub number: u32,
    pub bytes: u64,
    /// Without the surrounding `<` and `>`.
    pub message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    TooLarge { size: usize, max: usize },
    TooManyFiles { max: usize },
    TooManySegments { max: usize },
    NotAnNzb,
    NoFiles,
    Xml(String),
    BadSegment { file: usize, reason: String },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge { size, max } => write!(f, "NZB is {size} bytes; the limit is {max}"),
            Self::TooManyFiles { max } => write!(f, "NZB lists more than {max} files"),
            Self::TooManySegments { max } => write!(f, "NZB lists more than {max} segments"),
            Self::NotAnNzb => f.write_str("document has no <nzb> root element"),
            Self::NoFiles => f.write_str("NZB lists no files"),
            Self::Xml(msg) => write!(f, "invalid XML: {msg}"),
            Self::BadSegment { file, reason } => {
                write!(f, "file {} has a bad segment: {reason}", file + 1)
            }
        }
    }
}

impl std::error::Error for Error {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Capture {
    None,
    Meta,
    Group,
    Segment,
}

struct State {
    nzb: Nzb,
    seen_root: bool,
    segment_count: usize,
    capture: Capture,
    text: String,
    meta_type: String,
    segment: Option<(u32, u64)>,
    segment_errors: Vec<(usize, String)>,
    depth: usize,
}

pub fn parse(input: &[u8], limits: &Limits) -> Result<Nzb, Error> {
    if input.len() > limits.max_bytes {
        return Err(Error::TooLarge {
            size: input.len(),
            max: limits.max_bytes,
        });
    }
    // Posters still produce Latin-1 subjects; replacing bad bytes keeps the segments usable.
    let text = String::from_utf8_lossy(input);
    let mut reader = Reader::from_str(&text);
    let mut state = State {
        nzb: Nzb {
            meta: Vec::new(),
            files: Vec::new(),
        },
        seen_root: false,
        segment_count: 0,
        capture: Capture::None,
        text: String::new(),
        meta_type: String::new(),
        segment: None,
        segment_errors: Vec::new(),
        depth: 0,
    };

    loop {
        match reader.read_event().map_err(|e| Error::Xml(e.to_string()))? {
            Event::Start(e) => {
                state.depth += 1;
                start(&e, &mut state, limits)?;
            }
            Event::Empty(e) => {
                start(&e, &mut state, limits)?;
                end(e.local_name().as_ref(), &mut state);
            }
            Event::End(e) => {
                state.depth = state.depth.saturating_sub(1);
                end(e.local_name().as_ref(), &mut state);
            }
            Event::Text(t) if state.capture != Capture::None => {
                state.text.push_str(&t);
            }
            Event::CData(t) if state.capture != Capture::None => {
                state.text.push_str(&t);
            }
            Event::GeneralRef(r) if state.capture != Capture::None => state.text.push(resolve(&r)?),
            // quick-xml accepts a document that stops inside an element; a cut-off NZB is not usable.
            Event::Eof if state.depth > 0 => {
                return Err(Error::Xml(
                    "document ends before its elements are closed".to_owned(),
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if !state.seen_root {
        return Err(Error::NotAnNzb);
    }
    if let Some((file, reason)) = state.segment_errors.into_iter().next() {
        return Err(Error::BadSegment { file, reason });
    }
    let mut nzb = state.nzb;
    nzb.files.retain(|f| !f.segments.is_empty());
    if nzb.files.is_empty() {
        return Err(Error::NoFiles);
    }
    for file in &mut nzb.files {
        // Stable sort, then dedup keeps the first listing of a number in document order.
        file.segments.sort_by_key(|s| s.number);
        file.segments.dedup_by_key(|s| s.number);
    }
    Ok(nzb)
}

fn start(e: &BytesStart<'_>, state: &mut State, limits: &Limits) -> Result<(), Error> {
    match e.local_name().as_ref() {
        "nzb" => state.seen_root = true,
        "meta" => {
            state.meta_type = attr(e, "type")?.unwrap_or_default();
            state.capture = Capture::Meta;
            state.text.clear();
        }
        "file" => {
            if state.nzb.files.len() >= limits.max_files {
                return Err(Error::TooManyFiles {
                    max: limits.max_files,
                });
            }
            state.nzb.files.push(NzbFile {
                subject: attr(e, "subject")?.unwrap_or_default(),
                poster: attr(e, "poster")?,
                date: attr(e, "date")?.and_then(|d| d.trim().parse().ok()),
                groups: Vec::new(),
                segments: Vec::new(),
            });
        }
        "group" => {
            state.capture = Capture::Group;
            state.text.clear();
        }
        "segment" => {
            state.segment_count += 1;
            if state.segment_count > limits.max_segments {
                return Err(Error::TooManySegments {
                    max: limits.max_segments,
                });
            }
            let number = attr(e, "number")?
                .and_then(|n| n.trim().parse::<u32>().ok())
                .unwrap_or(0);
            let bytes = attr(e, "bytes")?
                .and_then(|b| b.trim().parse::<u64>().ok())
                .unwrap_or(0);
            state.segment = Some((number, bytes));
            state.capture = Capture::Segment;
            state.text.clear();
        }
        _ => {}
    }
    Ok(())
}

fn end(name: &str, state: &mut State) {
    let text = state.text.trim().to_owned();
    match (name, state.capture) {
        ("meta", Capture::Meta) => {
            let kind = std::mem::take(&mut state.meta_type);
            state.nzb.meta.push((kind, text));
        }
        ("group", Capture::Group) => {
            if let Some(file) = state.nzb.files.last_mut()
                && !text.is_empty()
            {
                file.groups.push(text);
            }
        }
        ("segment", Capture::Segment) => {
            let file_index = state.nzb.files.len().saturating_sub(1);
            let (number, bytes) = state.segment.take().unwrap_or((0, 0));
            let message_id = text
                .trim_start_matches('<')
                .trim_end_matches('>')
                .trim()
                .to_owned();
            let problem = if number == 0 {
                Some("segment number must be 1 or higher".to_owned())
            } else if message_id.is_empty() {
                Some(format!("segment {number} has no message id"))
            } else {
                None
            };
            match (problem, state.nzb.files.last_mut()) {
                (Some(reason), _) => state.segment_errors.push((file_index, reason)),
                (None, Some(file)) => file.segments.push(Segment {
                    number,
                    bytes,
                    message_id,
                }),
                (None, None) => {}
            }
        }
        _ => return,
    }
    state.capture = Capture::None;
    state.text.clear();
}

fn attr(e: &BytesStart<'_>, key: &str) -> Result<Option<String>, Error> {
    for attribute in e.attributes() {
        let attribute = attribute.map_err(|err| Error::Xml(err.to_string()))?;
        if attribute.key.local_name().as_ref() == key {
            let value = attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map_err(|err| Error::Xml(err.to_string()))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

fn resolve(reference: &BytesRef<'_>) -> Result<char, Error> {
    if let Some(c) = reference
        .resolve_char_ref()
        .map_err(|e| Error::Xml(e.to_string()))?
    {
        return Ok(c);
    }
    match &**reference {
        "amp" => Ok('&'),
        "lt" => Ok('<'),
        "gt" => Ok('>'),
        "quot" => Ok('"'),
        "apos" => Ok('\''),
        other => Err(Error::Xml(format!("entity &{other}; is not allowed"))),
    }
}

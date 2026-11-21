//! Release-name parsing.
//!
//! [`parse`] reads a scene or P2P release name such as `Dune.2021.2160p.WEB-DL.DDP5.1.Atmos.DV.HEVC-GROUP`
//! into a title, year, season and episode, resolution, source, codecs, HDR format, release group,
//! streaming service and a 0 to 103 quality score.
//!
//! The rules match the TypeScript parser this crate replaces, checked against the fixtures in
//! `fixtures/release-names/`. Matching uses ASCII semantics throughout (word boundaries, digits,
//! case folding), the way the JavaScript patterns behaved, so `Café2021` still yields the year 2021.
//! Release group tiers and streaming-service patterns come from the Dictionarry database.

mod groups;
mod score;
mod streaming;

use std::sync::LazyLock;

use regex::bytes::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

pub use groups::{Tier, TierSource, release_group_tier};
pub use score::quality_score;
pub use streaming::{StreamingService, detect_streaming_service};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Resolution {
    #[serde(rename = "2160p")]
    P2160,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "480p")]
    P480,
}

impl Resolution {
    fn from_token(t: &[u8]) -> Option<Self> {
        match t {
            b"2160p" => Some(Self::P2160),
            b"1080p" => Some(Self::P1080),
            b"720p" => Some(Self::P720),
            b"480p" => Some(Self::P480),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::P2160 => "2160p",
            Self::P1080 => "1080p",
            Self::P720 => "720p",
            Self::P480 => "480p",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Also `BDRip` and `BRRip`.
    Bluray,
    WebDl,
    WebRip,
    Hdtv,
    /// Only `DVDRip`; a bare `DVD` is not recognised.
    Dvd,
}

impl Source {
    fn from_token(t: &[u8]) -> Option<Self> {
        match t {
            b"bluray" | b"bdrip" | b"brrip" => Some(Self::Bluray),
            b"webdl" => Some(Self::WebDl),
            b"webrip" => Some(Self::WebRip),
            b"hdtv" => Some(Self::Hdtv),
            b"dvdrip" => Some(Self::Dvd),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bluray => "bluray",
            Self::WebDl => "webdl",
            Self::WebRip => "webrip",
            Self::Hdtv => "hdtv",
            Self::Dvd => "dvd",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    /// Also HEVC and H.265.
    X265,
    /// Also H.264.
    X264,
    Av1,
    Xvid,
}

impl VideoCodec {
    fn from_token(t: &[u8]) -> Option<Self> {
        match t {
            b"x265" | b"hevc" | b"h265" => Some(Self::X265),
            b"x264" | b"h264" => Some(Self::X264),
            b"av1" => Some(Self::Av1),
            b"xvid" => Some(Self::Xvid),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X265 => "x265",
            Self::X264 => "x264",
            Self::Av1 => "av1",
            Self::Xvid => "xvid",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    /// Wins over whatever codec carries it.
    Atmos,
    TrueHd,
    DtsX,
    /// Also DTS-HD MA.
    DtsHd,
    Dts,
    Ddp,
    Dd,
    Flac,
    Aac,
    Ac3,
    Opus,
}

impl AudioCodec {
    fn from_token(t: &[u8]) -> Option<Self> {
        match t {
            b"truehd" => Some(Self::TrueHd),
            b"dtsx" => Some(Self::DtsX),
            b"dtshd" | b"dtshdma" => Some(Self::DtsHd),
            b"dts" => Some(Self::Dts),
            b"ddp" => Some(Self::Ddp),
            b"dd" => Some(Self::Dd),
            b"flac" => Some(Self::Flac),
            b"aac" => Some(Self::Aac),
            b"ac3" => Some(Self::Ac3),
            b"opus" => Some(Self::Opus),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Atmos => "atmos",
            Self::TrueHd => "truehd",
            Self::DtsX => "dtsx",
            Self::DtsHd => "dtshd",
            Self::Dts => "dts",
            Self::Ddp => "ddp",
            Self::Dd => "dd",
            Self::Flac => "flac",
            Self::Aac => "aac",
            Self::Ac3 => "ac3",
            Self::Opus => "opus",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Hdr {
    #[serde(rename = "dv")]
    DolbyVision,
    #[serde(rename = "hdr10+")]
    Hdr10Plus,
    /// Also a bare `HDR`.
    #[serde(rename = "hdr10")]
    Hdr10,
    #[serde(rename = "hlg")]
    Hlg,
}

impl Hdr {
    fn from_token(t: &[u8]) -> Option<Self> {
        match t {
            b"dv" | b"dovi" | b"dolbyvision" => Some(Self::DolbyVision),
            b"hdr10+" => Some(Self::Hdr10Plus),
            b"hdr10" | b"hdr" => Some(Self::Hdr10),
            b"hlg" => Some(Self::Hlg),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DolbyVision => "dv",
            Self::Hdr10Plus => "hdr10+",
            Self::Hdr10 => "hdr10",
            Self::Hlg => "hlg",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParsedRelease {
    pub title: String,
    pub year: Option<u16>,
    pub season: Option<u8>,
    pub episode: Option<u8>,
    pub resolution: Option<Resolution>,
    pub source: Option<Source>,
    pub video_codec: Option<VideoCodec>,
    pub audio_codec: Option<AudioCodec>,
    pub hdr: Option<Hdr>,
    pub release_group: Option<String>,
    /// `None` both for an unknown group and for a name without a group.
    pub release_group_tier: Option<Tier>,
    /// Dictionarry abbreviation, such as `NF` or `AMAZ`.
    pub streaming_service: Option<String>,
    pub is_remux: bool,
    pub proper: bool,
    pub repack: bool,
    pub quality_score: u8,
}

/// JavaScript's non-Unicode regexes treat `\b`, `\d`, `\s` and case folding as ASCII. Matching
/// bytes with Unicode off reproduces that, and match offsets always land on ASCII bytes, so they
/// are valid `str` boundaries.
pub(crate) fn ascii_regex(pattern: &str) -> Regex {
    RegexBuilder::new(pattern)
        .unicode(false)
        .case_insensitive(true)
        .build()
        .unwrap_or_else(|e| panic!("built-in pattern {pattern:?} must compile: {e}"))
}

struct Patterns {
    resolution: Regex,
    source: Regex,
    remux: Regex,
    video_codec: Regex,
    atmos: Regex,
    audio_codec: Regex,
    hdr: Regex,
    year: Regex,
    season_episode: Regex,
    season_only: Regex,
    release_group: Regex,
    proper: Regex,
    repack: Regex,
}

static PATTERNS: LazyLock<Patterns> = LazyLock::new(|| Patterns {
    resolution: ascii_regex(r"\b(2160p|1080p|720p|480p)\b"),
    source: ascii_regex(r"\b(blu-?ray|web-?dl|web-?rip|hdtv|dvd-?rip|bdrip|brrip)\b"),
    remux: ascii_regex(r"\bremux\b"),
    video_codec: ascii_regex(r"\b(x265|x\.?264|hevc|h\.?265|h\.?264|av1|xvid)\b"),
    atmos: ascii_regex(r"\batmos\b"),
    audio_codec: ascii_regex(r"\b(truehd|dts-?x|dts-?hd(?:\s*ma)?|dts|ddp|dd|flac|aac|ac3|opus)\b"),
    hdr: ascii_regex(r"\b(dv|dovi|dolby[- ]?vision|hdr10\+|hdr10|hdr|hlg)\b"),
    year: ascii_regex(r"\b(19\d{2}|20\d{2})\b"),
    season_episode: ascii_regex(r"\bs(\d{1,2})e(\d{1,2})\b"),
    season_only: ascii_regex(r"\bs(\d{1,2})\b"),
    // `[^\n\r]` stands in for JavaScript's `.`, which stops at both line endings.
    release_group: ascii_regex(r"-([a-z0-9]+)(?:\[[^\n\r]*?\])?$"),
    proper: ascii_regex(r"\b(proper|real)\b"),
    repack: ascii_regex(r"\brepack\b"),
});

/// The first capture group, lowercased with the given separators removed, so `Blu-Ray` and
/// `bluray` map to the same variant.
fn token(re: &Regex, haystack: &[u8], strip: &[u8]) -> Option<Vec<u8>> {
    let group = re.captures(haystack)?.get(1)?;
    Some(
        group
            .as_bytes()
            .iter()
            .filter(|b| !strip.contains(b))
            .map(u8::to_ascii_lowercase)
            .collect(),
    )
}

fn number<T: std::str::FromStr>(bytes: &[u8]) -> Option<T> {
    std::str::from_utf8(bytes).ok()?.parse().ok()
}

#[must_use]
pub fn parse(name: &str) -> ParsedRelease {
    let p = &*PATTERNS;
    let normalized = name.replace('_', ".");
    let h = normalized.as_bytes();

    let resolution = token(&p.resolution, h, b"").and_then(|t| Resolution::from_token(&t));
    let source = token(&p.source, h, b".-").and_then(|t| Source::from_token(&t));
    let video_codec = token(&p.video_codec, h, b".").and_then(|t| VideoCodec::from_token(&t));
    // Atmos rides on top of TrueHD or DD+, so it is checked before the codec it travels with.
    let audio_codec = if p.atmos.is_match(h) {
        Some(AudioCodec::Atmos)
    } else {
        token(&p.audio_codec, h, b".- \t\n\x0b\x0c\r").and_then(|t| AudioCodec::from_token(&t))
    };
    let hdr = token(&p.hdr, h, b"- ").and_then(|t| Hdr::from_token(&t));

    let year_match = p.year.captures(h);
    let year = year_match
        .as_ref()
        .and_then(|c| number(c.get(1)?.as_bytes()));

    let se_match = p.season_episode.captures(h);
    let (season, episode) = match &se_match {
        Some(c) => (
            c.get(1).and_then(|m| number(m.as_bytes())),
            c.get(2).and_then(|m| number(m.as_bytes())),
        ),
        None => (
            p.season_only
                .captures(h)
                .and_then(|c| number(c.get(1)?.as_bytes())),
            None,
        ),
    };

    let release_group = p
        .release_group
        .captures(h)
        .and_then(|c| c.get(1))
        .map(|m| String::from_utf8_lossy(m.as_bytes()).into_owned());
    let release_group_tier = release_group.as_deref().and_then(|group| {
        let tier_source = match source {
            Some(Source::WebDl | Source::WebRip) => TierSource::WebDl,
            Some(Source::Bluray) => TierSource::Bluray,
            Some(Source::Dvd) => TierSource::Dvd,
            Some(Source::Hdtv) | None => TierSource::Any,
        };
        release_group_tier(group, resolution.map(Resolution::as_str), tier_source)
    });

    // A marker at the very start never ends the title, so a name like `1917.2019...` keeps a
    // title. A season without an episode (`S02`) never ends it either; both mirror the old parser.
    let title_end = [
        year_match.and_then(|c| c.get(0)),
        se_match.and_then(|c| c.get(0)),
    ]
    .into_iter()
    .flatten()
    .map(|m| m.start())
    .filter(|&start| start > 0)
    .min()
    .unwrap_or(normalized.len());
    let title = normalized
        .get(..title_end)
        .unwrap_or(&normalized)
        .replace('.', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let mut parsed = ParsedRelease {
        title,
        year,
        season,
        episode,
        resolution,
        source,
        video_codec,
        audio_codec,
        hdr,
        release_group,
        release_group_tier,
        streaming_service: detect_streaming_service(&normalized).map(|s| s.abbreviation.to_owned()),
        is_remux: p.remux.is_match(h),
        proper: p.proper.is_match(h),
        repack: p.repack.is_match(h),
        quality_score: 0,
    };
    parsed.quality_score = quality_score(&parsed);
    parsed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_year_at_the_start_stays_in_the_title() {
        let parsed = parse("1917.2019.1080p.BluRay.x264-GROUP");
        assert_eq!(parsed.year, Some(1917));
        assert!(parsed.title.starts_with("1917 2019"));
    }

    #[test]
    fn three_digit_seasons_are_not_episodes() {
        let parsed = parse("Show.S100E01.720p.HDTV.x264-GROUP");
        assert_eq!((parsed.season, parsed.episode), (None, None));
    }

    #[test]
    fn non_ascii_letters_are_not_word_characters() {
        assert_eq!(parse("Caf\u{e9}2021.1080p").year, Some(2021));
    }
}

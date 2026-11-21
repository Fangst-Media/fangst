use std::sync::LazyLock;

use regex::bytes::Regex;

use crate::ascii_regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamingService {
    pub name: &'static str,
    /// Dictionarry's short code, such as `NF`. Several entries can share one.
    pub abbreviation: &'static str,
}

/// One row of the Dictionarry streaming-service table. The source patterns use lookarounds,
/// which the `regex` crate lacks. Lookaheads are folded into `pattern`, which is equivalent when
/// only asking whether a name matches. A lookbehind becomes `not_after`: a pattern that must not
/// end right where the match starts.
struct Row {
    name: &'static str,
    abbreviation: &'static str,
    pattern: &'static str,
    not_after: Option<&'static str>,
}

const fn row(name: &'static str, abbreviation: &'static str, pattern: &'static str) -> Row {
    Row {
        name,
        abbreviation,
        pattern,
        not_after: None,
    }
}

// Order matters: the first service that matches wins. `[^\n\r]` stands in for JavaScript's `.`.
const ROWS: &[Row] = &[
    row(
        "Amazon Prime",
        "AMAZ",
        r"\b(?:AMZN\b|AMAZON\b\s*[^\n\r](?:WEB-?DL|WEBRIP))",
    ),
    row("Apple TV+", "ATVP", r"\b(ATVP|ATV|APTV|Apple TV\+)\s*\b"),
    row("BBC iPlayer", "BBCI", r"\b(iP)\b"),
    row(
        "Bravia Core",
        "BRAV",
        r"\b(?:BCORE\b|CORE\b\s*[^\n\r](?:WEB-?DL|WEBRIP))",
    ),
    row(
        "Crave",
        "CRAV",
        r"\b(?:CRAV\b|CRAVE\b\s*[^\n\r](?:WEB-?DL|WEBRIP))",
    ),
    row("Criterion Channel", "CRIT", r"\b(CRiT)\b"),
    row("Disney+", "DSNP", r"\b(dsnp|dsny(p)?|disney\+)\s*\b"),
    row("Dropout TV", "DROP", r"\b(DRPO)\b"),
    row(
        "Google TV",
        "GOOG",
        r"\b(PLAY)\b[ ._-]web[ ._-]?(dl|rip)?\b",
    ),
    row("Google TV Rename", "GOOG", r"\[(PLAY)\b|\b(PLAY)\]"),
    row(
        "HBO Max",
        "HBOM",
        r"\b(?:HMAX\b|HBOM\b|HBO[ ._-]?MAX\b\s*[^\n\r](?:WEB-?DL|WEBRIP))",
    ),
    row("Hotstar", "HOTS", r"\b(HTSR)\b"),
    row("Hulu", "HULU", r"\b(Hulu)\b"),
    row("iQIYI", "IQIYI", r"\b(IQIYI|IQ)\b"),
    row("iTunes", "IT", r"\b(it|itunes)\b[ ._-]web[ ._-]?(dl|rip)\b"),
    row("iTunes Rename", "ITUN", r"\[(iT)\b|\b(iT)\]"),
    row("KCW", "KCW", r"\b(KCW|Kocowa)\b"),
    Row {
        name: "Max",
        abbreviation: "MAX",
        pattern: r"\bmax\b[ ._-]web[ ._-]?(dl|rip)\b",
        not_after: Some(r"hbo[ ._-]"),
    },
    row("Max Rename", "MAXR", r"\[(MAX)\b|\b(MAX)\]"),
    // `MA` also appears in `DTS-HD MA`, which is audio, not the service.
    Row {
        name: "Movies Anywhere",
        abbreviation: "MOVI",
        pattern: r"\bma\b[^\n\r]*\bweb[ ._-]?(dl|rip)\b",
        not_after: Some(r"dts[ .-]?hd[ .-]?"),
    },
    row("Mubi", "MUBI", r"\b(MUBI)\b"),
    row("Netflix", "NF", r"\b(NF|Netflix)\b"),
    row("Now", "NOW", r"\b(now)\b[ ._-]web[ ._-]?(dl|rip)?\b"),
    row("Now Rename", "NOWR", r"\[(NOW)\b|\b(NOW)\]"),
    row("Paramount+", "PMTP", r"\b(PMTP|Paramount Plus)\b"),
    row(
        "Peacock TV",
        "PEAC",
        r"\b(?:PCOK\b|Peacock\b\s*[^\n\r](?:WEB-?DL|WEBRIP))",
    ),
    row("Roku", "ROKU", r"\b(ROKU)\b"),
    row(
        "Showtime",
        "SHO",
        r"\b(sho|showtime)\b[ ._-]web[ ._-]?(dl|rip)?\b",
    ),
    row("Showtime Rename", "SHOW", r"\[(SHO)\b|\b(SHO)\]"),
    row("Stan", "STAN", r"\b(stan)\b[ ._-]web[ ._-]?(dl|rip)?\b"),
    row("Stan Rename", "STAN", r"\[(STAN)\b|\b(STAN)\]"),
    row("TVING", "TVIN", r"\b(TVING)\b"),
    row("Viki", "VIKI", r"\b(Viki)\b"),
    row("Viu", "VIU", r"\b(Viu)\b"),
    row("Wavve", "WAVV", r"\b(Wavve)\b"),
    row("WeTV", "WETV", r"\b(WeTV)\b"),
    row("Youku", "YOUKU", r"\b(Youku|YK)\b"),
];

struct Compiled {
    service: StreamingService,
    pattern: Regex,
    not_after: Option<Regex>,
}

impl Compiled {
    fn matches(&self, haystack: &[u8]) -> bool {
        let Some(not_after) = &self.not_after else {
            return self.pattern.is_match(haystack);
        };
        // Retry from every candidate start, not from the end of the last match, so a rejected
        // `DTS-HD MA` cannot hide a later `MA` that the greedy match swallowed.
        let mut at = 0;
        while let Some(m) = self.pattern.find_at(haystack, at) {
            if !not_after.is_match(&haystack[..m.start()]) {
                return true;
            }
            at = m.start() + 1;
        }
        false
    }
}

static SERVICES: LazyLock<Vec<Compiled>> = LazyLock::new(|| {
    ROWS.iter()
        .map(|r| Compiled {
            service: StreamingService {
                name: r.name,
                abbreviation: r.abbreviation,
            },
            pattern: ascii_regex(r.pattern),
            not_after: r.not_after.map(|p| ascii_regex(&format!(r"(?:{p})\z"))),
        })
        .collect()
});

/// The first streaming service named in a release name.
#[must_use]
pub fn detect_streaming_service(name: &str) -> Option<&'static StreamingService> {
    let haystack = name.as_bytes();
    SERVICES
        .iter()
        .find(|c| c.matches(haystack))
        .map(|c| &c.service)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abbreviation(name: &str) -> Option<&'static str> {
        detect_streaming_service(name).map(|s| s.abbreviation)
    }

    #[test]
    fn a_rejected_dts_hd_ma_does_not_hide_a_later_ma() {
        assert_eq!(abbreviation("Movie.DTS-HD.MA.5.1.MA.WEB-DL"), Some("MOVI"));
        assert_eq!(abbreviation("Movie.DTS-HD.MA.WEB-DL"), None);
    }

    #[test]
    fn max_after_hbo_is_not_max() {
        assert_eq!(abbreviation("Show.HBO MAX WEB-DL"), Some("HBOM"));
        assert_eq!(abbreviation("Show.hbo-max.webrip"), Some("HBOM"));
        assert_eq!(abbreviation("Show.MAX.WEBRip"), Some("MAX"));
    }

    #[test]
    fn lookahead_services_need_the_web_tag() {
        assert_eq!(abbreviation("Movie.Amazon.Prime"), None);
        assert_eq!(abbreviation("Movie.AMAZON.WEBRip"), Some("AMAZ"));
    }
}

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

/// Dictionarry's quality tier for a release group; tier 1 is the most trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(into = "u8", try_from = "u8")]
pub enum Tier {
    One,
    Two,
    Three,
}

impl From<Tier> for u8 {
    fn from(tier: Tier) -> Self {
        match tier {
            Tier::One => 1,
            Tier::Two => 2,
            Tier::Three => 3,
        }
    }
}

impl TryFrom<u8> for Tier {
    type Error = String;

    fn try_from(n: u8) -> Result<Self, Self::Error> {
        match n {
            1 => Ok(Self::One),
            2 => Ok(Self::Two),
            3 => Ok(Self::Three),
            _ => Err(format!("release group tier must be 1, 2 or 3, not {n}")),
        }
    }
}

/// The source families tiers are ranked within. `WEBRip` counts as `WebDl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TierSource {
    Any,
    WebDl,
    Bluray,
    Dvd,
}

impl TierSource {
    fn from_data(s: &str) -> Option<Self> {
        match s {
            "any" => Some(Self::Any),
            "web_dl" => Some(Self::WebDl),
            "bluray" => Some(Self::Bluray),
            "dvd" => Some(Self::Dvd),
            _ => None,
        }
    }
}

struct Entry {
    tier: Tier,
    /// `None` matches any resolution.
    resolution: Option<Box<str>>,
    source: TierSource,
}

const DATA: &str = include_str!("../data/release-groups.tsv");

fn parse_row(line: &str) -> Option<(String, Entry)> {
    let mut cols = line.split('\t');
    let key = cols.next()?.to_owned();
    let _display_name = cols.next()?;
    let tier = cols.next()?.parse::<u8>().ok()?.try_into().ok()?;
    let resolution = match cols.next()? {
        "any" => None,
        r => Some(r.into()),
    };
    let source = TierSource::from_data(cols.next()?)?;
    Some((
        key,
        Entry {
            tier,
            resolution,
            source,
        },
    ))
}

fn data_rows() -> impl Iterator<Item = &'static str> {
    DATA.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
}

/// Rows keep their file order: the first matching row decides.
static TIERS: LazyLock<HashMap<String, Vec<Entry>>> = LazyLock::new(|| {
    let mut map: HashMap<String, Vec<Entry>> = HashMap::new();
    for (key, entry) in data_rows().filter_map(parse_row) {
        map.entry(key).or_default().push(entry);
    }
    map
});

/// Looks a group up case-insensitively. The first row matching both resolution and source
/// decides; a known group with no matching row falls back to its first row, so known groups always
/// get a tier.
#[must_use]
pub fn release_group_tier(
    group: &str,
    resolution: Option<&str>,
    source: TierSource,
) -> Option<Tier> {
    let entries = TIERS.get(&group.to_lowercase())?;
    let matching = entries.iter().find(|e| {
        e.resolution
            .as_deref()
            .is_none_or(|r| Some(r) == resolution)
            && (e.source == TierSource::Any || e.source == source)
    });
    matching.or_else(|| entries.first()).map(|e| e.tier)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_data_row_parses() {
        let bad: Vec<_> = data_rows().filter(|l| parse_row(l).is_none()).collect();
        assert!(bad.is_empty(), "unparseable rows: {bad:?}");
        assert!(TIERS.len() > 100);
    }

    // The TypeScript parser looked groups up on a plain object and threw on these names.
    #[test]
    fn object_prototype_names_are_unknown_groups() {
        for group in ["constructor", "__proto__", "toString", "hasOwnProperty"] {
            assert_eq!(release_group_tier(group, None, TierSource::Any), None);
        }
    }

    #[test]
    fn keys_are_lowercase() {
        assert!(TIERS.keys().all(|k| *k == k.to_lowercase()));
    }
}

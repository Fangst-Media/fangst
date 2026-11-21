use crate::{AudioCodec, Hdr, ParsedRelease, Resolution, Source, Tier};

/// Resolution up to 40, source up to 25 (a remux always gets 25), group tier up to 20, audio up
/// to 10, HDR up to 5, plus 2 for PROPER and 1 for REPACK. The maximum is 103, not 100.
#[must_use]
pub fn quality_score(release: &ParsedRelease) -> u8 {
    let resolution = match release.resolution {
        Some(Resolution::P2160) => 40,
        Some(Resolution::P1080) => 30,
        Some(Resolution::P720) => 15,
        Some(Resolution::P480) => 5,
        None => 0,
    };
    let source = if release.is_remux {
        25
    } else {
        match release.source {
            Some(Source::Bluray) => 20,
            Some(Source::WebDl) => 15,
            Some(Source::WebRip) => 10,
            Some(Source::Hdtv) => 5,
            Some(Source::Dvd) => 2,
            None => 0,
        }
    };
    let tier = match release.release_group_tier {
        Some(Tier::One) => 20,
        Some(Tier::Two) => 12,
        Some(Tier::Three) => 5,
        None => 0,
    };
    let audio = match release.audio_codec {
        Some(AudioCodec::Atmos) => 10,
        Some(AudioCodec::TrueHd | AudioCodec::DtsX) => 9,
        Some(AudioCodec::DtsHd) => 8,
        Some(AudioCodec::Dts) => 6,
        Some(AudioCodec::Ddp) => 5,
        Some(AudioCodec::Dd | AudioCodec::Flac) => 4,
        Some(AudioCodec::Aac | AudioCodec::Opus) => 3,
        Some(AudioCodec::Ac3) => 2,
        None => 0,
    };
    let hdr = match release.hdr {
        Some(Hdr::DolbyVision) => 5,
        Some(Hdr::Hdr10Plus) => 4,
        Some(Hdr::Hdr10) => 3,
        Some(Hdr::Hlg) => 2,
        None => 0,
    };
    resolution
        + source
        + tier
        + audio
        + hdr
        + if release.proper { 2 } else { 0 }
        + u8::from(release.repack)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_best_possible_release_scores_103() {
        let best = ParsedRelease {
            resolution: Some(Resolution::P2160),
            is_remux: true,
            release_group_tier: Some(Tier::One),
            audio_codec: Some(AudioCodec::Atmos),
            hdr: Some(Hdr::DolbyVision),
            proper: true,
            repack: true,
            ..ParsedRelease::default()
        };
        assert_eq!(quality_score(&best), 103);
    }
}

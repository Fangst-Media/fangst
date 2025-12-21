/**
 * Quality Scoring Module
 *
 * Calculates an overall quality score (0-100) for parsed releases based on:
 * - Resolution (max 40 points)
 * - Source quality (max 25 points)
 * - Release group tier (max 20 points)
 * - Audio codec (max 10 points)
 * - HDR format (max 5 points)
 */

import type { ParsedRelease } from './index'

const RESOLUTION_SCORES: Record<string, number> = {
  '2160p': 40,
  '1080p': 30,
  '720p': 15,
  '480p': 5,
}

const SOURCE_SCORES: Record<string, number> = {
  remux: 25,
  bluray: 20,
  webdl: 15,
  webrip: 10,
  hdtv: 5,
  dvd: 2,
}

const TIER_SCORES: Record<number, number> = {
  1: 20,
  2: 12,
  3: 5,
}

const AUDIO_SCORES: Record<string, number> = {
  atmos: 10,
  truehd: 9,
  dtsx: 9,
  dtshd: 8,
  dts: 6,
  ddp: 5,
  dd: 4,
  flac: 4,
  aac: 3,
  ac3: 2,
  opus: 3,
}

const HDR_SCORES: Record<string, number> = {
  dv: 5,
  'hdr10+': 4,
  hdr10: 3,
  hlg: 2,
}

export function calculateQualityScore(release: ParsedRelease): number {
  let score = 0

  // Resolution (max 40)
  if (release.resolution) {
    score += RESOLUTION_SCORES[release.resolution] ?? 0
  }

  // Source quality (max 25)
  if (release.isRemux) {
    score += 25 // Remux is always max source score
  } else if (release.source) {
    score += SOURCE_SCORES[release.source] ?? 0
  }

  // Release group tier (max 20)
  if (release.releaseGroupTier) {
    score += TIER_SCORES[release.releaseGroupTier] ?? 0
  }

  // Audio codec (max 10)
  if (release.audioCodec) {
    score += AUDIO_SCORES[release.audioCodec] ?? 0
  }

  // HDR bonus (max 5)
  if (release.hdr) {
    score += HDR_SCORES[release.hdr] ?? 0
  }

  // Bonuses
  if (release.proper) score += 2
  if (release.repack) score += 1

  return score
}

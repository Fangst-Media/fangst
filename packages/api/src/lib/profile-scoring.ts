import type { ParsedRelease } from '@jaeger/parser'
import type { QualityProfile } from '@jaeger/shared/types/profiles'

const RESOLUTION_ORDER = ['2160p', '1080p', '720p', '480p']

export type ScoredRelease = {
  baseScore: number
  profileBonus: number
  totalScore: number
  rejected: boolean
  rejectReason?: string
}

function resolutionIndex(res: string): number {
  const idx = RESOLUTION_ORDER.indexOf(res)
  return idx === -1 ? RESOLUTION_ORDER.length : idx
}

function preferenceBonus(value: string | undefined, preferences: string[] | undefined, maxPoints: number): number {
  if (!value || !preferences || preferences.length === 0) return 0
  const idx = preferences.indexOf(value)
  if (idx === -1) return 0
  // First preference gets max, decays linearly
  const step = maxPoints / preferences.length
  return Math.round(maxPoints - idx * step)
}

export function scoreReleaseWithProfile(
  parsed: ParsedRelease,
  profile: QualityProfile
): ScoredRelease {
  const baseScore = parsed.qualityScore ?? 0

  // Hard reject: below minimum resolution
  if (parsed.resolution) {
    const releaseIdx = resolutionIndex(parsed.resolution)
    const minIdx = resolutionIndex(profile.minResolution)
    if (releaseIdx > minIdx) {
      return { baseScore, profileBonus: 0, totalScore: 0, rejected: true, rejectReason: `Below minimum resolution (${profile.minResolution})` }
    }
  } else {
    // No resolution detected — reject if profile requires a minimum above 480p
    if (profile.minResolution !== '720p') {
      return { baseScore, profileBonus: 0, totalScore: 0, rejected: true, rejectReason: 'Resolution not detected' }
    }
  }

  // Hard reject: HDR required but not present
  if (profile.requireHdr && !parsed.hdr) {
    return { baseScore, profileBonus: 0, totalScore: 0, rejected: true, rejectReason: 'HDR required but not detected' }
  }

  // Preference bonuses
  let profileBonus = 0

  // Resolution match: +10 if matches preferred
  if (profile.preferredResolution && parsed.resolution === profile.preferredResolution) {
    profileBonus += 10
  }

  // Source preference: max +8
  const sourceValue = parsed.isRemux ? 'remux' : parsed.source
  profileBonus += preferenceBonus(sourceValue, profile.preferredSources, 8)

  // HDR preference: max +5
  profileBonus += preferenceBonus(parsed.hdr, profile.preferredHdr, 5)

  // Audio preference: max +5
  profileBonus += preferenceBonus(parsed.audioCodec, profile.preferredAudio, 5)

  const totalScore = baseScore + profileBonus

  return { baseScore, profileBonus, totalScore, rejected: false }
}

/**
 * Check if a download's score meets the profile's cutoff (no upgrade needed).
 */
export function meetsCutoff(downloadScore: number, profile: QualityProfile): boolean {
  return downloadScore >= profile.cutoffScore
}

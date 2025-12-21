export interface SizeFilterConfig {
  enabled: boolean
  minMbPerMin720p: number
  minMbPerMin1080p: number
  minMbPerMin2160p: number
}

export function isReleaseTooSmall(
  sizeBytes: number,
  runtimeMinutes: number,
  resolution: string | undefined,
  config: SizeFilterConfig
): boolean {
  if (!config.enabled || !runtimeMinutes || runtimeMinutes <= 0) return false

  const sizeMB = sizeBytes / 1024 / 1024
  const mbPerMin = sizeMB / runtimeMinutes

  let threshold = config.minMbPerMin1080p
  if (resolution === '2160p') threshold = config.minMbPerMin2160p
  else if (resolution === '720p') threshold = config.minMbPerMin720p

  return mbPerMin < threshold
}

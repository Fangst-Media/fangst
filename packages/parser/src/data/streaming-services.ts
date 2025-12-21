// Auto-generated from Dictionarry database
// Source: https://github.com/Dictionarry-Hub/database
// Generated: 2025-12-20T19:33:38.485Z

export interface StreamingService {
  name: string
  abbreviation: string
  pattern: RegExp
}

export const streamingServices: StreamingService[] = [
  { name: 'Amazon Prime', abbreviation: 'AMAZ', pattern: /\b(?:AMZN|(?:AMAZON)(?=\s*.(?:WEB-?DL|WEBRIP)))\b/i },
  { name: 'Apple TV+', abbreviation: 'ATVP', pattern: /\b(ATVP|ATV|APTV|Apple TV\+)\s*\b/i },
  { name: 'BBC iPlayer', abbreviation: 'BBCI', pattern: /\b(iP)\b/i },
  { name: 'Bravia Core', abbreviation: 'BRAV', pattern: /\b(?:BCORE|(?:CORE)(?=\s*.(?:WEB-?DL|WEBRIP)))\b/i },
  { name: 'Crave', abbreviation: 'CRAV', pattern: /\b(?:CRAV|(?:CRAVE)(?=\s*.(?:WEB-?DL|WEBRIP)))\b/i },
  { name: 'Criterion Channel', abbreviation: 'CRIT', pattern: /\b(CRiT)\b/i },
  { name: 'Disney+', abbreviation: 'DSNP', pattern: /\b(dsnp|dsny(p)?|disney\+)\s*\b/i },
  { name: 'Dropout TV', abbreviation: 'DROP', pattern: /\b(DRPO)\b/i },
  { name: 'Google TV', abbreviation: 'GOOG', pattern: /\b(PLAY)\b[ ._-]web[ ._-]?(dl|rip)?\b/i },
  { name: 'Google TV Rename', abbreviation: 'GOOG', pattern: /\[(PLAY)\b|\b(PLAY)\]/i },
  { name: 'HBO Max', abbreviation: 'HBOM', pattern: /\b(?:HMAX|HBOM|(?:HBO[ ._-]?MAX)(?=\s*.(?:WEB-?DL|WEBRIP)))\b/i },
  { name: 'Hotstar', abbreviation: 'HOTS', pattern: /\b(HTSR)\b/i },
  { name: 'Hulu', abbreviation: 'HULU', pattern: /\b(Hulu)\b/i },
  { name: 'iQIYI', abbreviation: 'IQIYI', pattern: /\b(IQIYI|IQ)\b/i },
  { name: 'iTunes', abbreviation: 'IT', pattern: /\b(it|itunes)\b(?=[ ._-]web[ ._-]?(dl|rip)\b)/i },
  { name: 'iTunes Rename', abbreviation: 'ITUN', pattern: /\[(iT)\b|\b(iT)\]/i },
  { name: 'KCW', abbreviation: 'KCW', pattern: /\b(KCW|Kocowa)\b/i },
  { name: 'Max', abbreviation: 'MAX', pattern: /\b((?<!hbo[ ._-])max)\b(?=[ ._-]web[ ._-]?(dl|rip)\b)/i },
  { name: 'Max Rename', abbreviation: 'MAXR', pattern: /\[(MAX)\b|\b(MAX)\]/i },
  { name: 'Movies Anywhere', abbreviation: 'MOVI', pattern: /\b(?<!dts[ .-]?hd[ .-]?)ma\b(?=.*\bweb[ ._-]?(dl|rip)\b)/i },
  { name: 'Mubi', abbreviation: 'MUBI', pattern: /\b(MUBI)\b/i },
  { name: 'Netflix', abbreviation: 'NF', pattern: /\b(NF|Netflix)\b/i },
  { name: 'Now', abbreviation: 'NOW', pattern: /\b(now)\b[ ._-]web[ ._-]?(dl|rip)?\b/i },
  { name: 'Now Rename', abbreviation: 'NOWR', pattern: /\[(NOW)\b|\b(NOW)\]/i },
  { name: 'Paramount+', abbreviation: 'PMTP', pattern: /\b(PMTP|Paramount Plus)\b/i },
  { name: 'Peacock TV', abbreviation: 'PEAC', pattern: /\b(?:PCOK|(?:Peacock)(?=\s*.(?:WEB-?DL|WEBRIP)))\b/i },
  { name: 'Roku', abbreviation: 'ROKU', pattern: /\b(ROKU)\b/i },
  { name: 'Showtime', abbreviation: 'SHO', pattern: /\b(sho|showtime)\b[ ._-]web[ ._-]?(dl|rip)?\b/i },
  { name: 'Showtime Rename', abbreviation: 'SHOW', pattern: /\[(SHO)\b|\b(SHO)\]/i },
  { name: 'Stan', abbreviation: 'STAN', pattern: /\b(stan)\b[ ._-]web[ ._-]?(dl|rip)?\b/i },
  { name: 'Stan Rename', abbreviation: 'STAN', pattern: /\[(STAN)\b|\b(STAN)\]/i },
  { name: 'TVING', abbreviation: 'TVIN', pattern: /\b(TVING)\b/i },
  { name: 'Viki', abbreviation: 'VIKI', pattern: /\b(Viki)\b/i },
  { name: 'Viu', abbreviation: 'VIU', pattern: /\b(Viu)\b/i },
  { name: 'Wavve', abbreviation: 'WAVV', pattern: /\b(Wavve)\b/i },
  { name: 'WeTV', abbreviation: 'WETV', pattern: /\b(WeTV)\b/i },
  { name: 'Youku', abbreviation: 'YOUKU', pattern: /\b(Youku|YK)\b/i },
]

export function detectStreamingService(name: string): string | undefined {
  for (const service of streamingServices) {
    if (service.pattern.test(name)) {
      return service.abbreviation
    }
  }
  return undefined
}

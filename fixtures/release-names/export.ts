// Regenerates the release-name fixtures and the release group tier table for crates/fangst-release
// by running the old TypeScript parser, so the Rust port is checked against what it really did.
// Run from the repo root: bun run fixtures/release-names/export.ts

import { writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { releaseGroupTiers } from '../../packages/parser/src/data/release-groups'
import {
  detectStreamingService,
  getReleaseGroupTier,
  parseReleaseName,
} from '../../packages/parser/src/index'

const here = dirname(fileURLToPath(import.meta.url))
const crateData = join(here, '../../crates/fangst-release/data')

// Every input used by packages/parser/src/index.test.ts.
const testNames = [
  'The.Matrix.1999.1080p.BluRay.x264-GROUP',
  'Breaking.Bad.S01E01.1080p.WEB-DL.DD5.1.x264-GROUP',
  'Dune.2021.2160p.WEB-DL.DDP5.1.Atmos.DV.HDR10.HEVC-GROUP',
  'Movie.2023.1080p.PROPER.WEB-DL.x264-GROUP',
  'Movie.2023.1080p.REPACK.WEB-DL.x264-GROUP',
  'Movie.2023.2160p.UHD.BluRay.Remux.TrueHD.7.1.Atmos-FraMeSToR',
  'Movie.2024.1080p.NF.WEB-DL.DDP5.1-GROUP',
  'Movie.2024.2160p.AMZN.WEB-DL.DDP5.1.DV.HDR-GROUP',
  'Show.S01E01.1080p.DSNP.WEB-DL.DDP5.1-GROUP',
  'Show.S01E01.2160p.ATVP.WEB-DL.DDP5.1.Atmos.DV-GROUP',
  'Movie.2024.1080p.BluRay.x264-GROUP',
  'Movie.2024.1080p.BluRay.x264-hallowed',
  'Movie.2024.1080p.NF.WEB-DL.DDP5.1-FLUX',
  'Movie.2024.1080p.BluRay.x264-BHDStudio',
  'Movie.2024.1080p.BluRay.x264-UNKNOWNGROUP',
  'Movie.2024.2160p.UHD.BluRay.Remux.TrueHD.Atmos-FraMeSToR',
  'Movie.2024.2160p.WEB-DL.DDP5.1-GROUP',
  'Movie.2024.1080p.WEB-DL.DDP5.1-GROUP',
  'Movie.2024.1080p.WEB-DL.x264-GROUP',
  'Movie.2024.1080p.PROPER.WEB-DL.x264-GROUP',
]

// Names beyond the tests that reach the remaining branches, quirks included.
const extraNames = [
  '',
  'S01E01',
  'Show.Name.S02.1080p.BluRay.x264-GROUP',
  'Show_Name_S03E10_720p_HDTV_x264-GROUP',
  'Show Name S03E10 720p HDTV x264-GROUP',
  'Old.Movie.1985.DVDRip.XviD-GROUP',
  'Movie.2019.1080p.BRRip.x264.AAC-GROUP',
  'Movie.2019.720p.BDRip.x264.AC3-GROUP',
  'Movie.2019.480p.WEBRip.x264.Opus-GROUP',
  'Movie.2020.1080p.WEB-DL.H.264.FLAC.2.0-GROUP',
  'Movie.2020.1080p.WEB-DL.H.265.Opus-GROUP',
  'Movie.2021.2160p.BluRay.AV1.DTS-X-GROUP',
  'Movie.2021.1080p.BluRay.DTS-HD.MA.5.1.x264-GROUP',
  'Movie.2021.1080p.BluRay.DTS-HD MA.5.1.x264-GROUP',
  'Movie.2021.1080p.BluRay.DTS.5.1.x264-GROUP',
  'Movie.2021.1080p.BluRay.TrueHD.7.1.x264-GROUP',
  'Movie.2021.1080p.WEB-DL.DD.5.1.x264-GROUP',
  'Movie.2021.1080p.WEB-DL.DDP.5.1.x264-GROUP',
  'Movie.2021.1080p.MA.WEB-DL.DDP5.1.H.264-GROUP',
  'Movie.2021.1080p.DTS-HD.MA.WEB-DL.x264-GROUP',
  'Show.S01E05.1080p.HMAX.WEB-DL.DD5.1.x264-GROUP',
  'Show.S01E05.1080p.HBO.MAX.WEB-DL.x264-GROUP',
  'Show.S01E05.1080p.MAX.WEB-DL.DDP5.1.x264-GROUP',
  'Movie.2022.1080p.AMAZON.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.Amazon.Prime.x264-GROUP',
  'Movie.2022.1080p.iT.WEB-DL.AAC2.0.H.264-GROUP',
  'Movie.2022.1080p.WEB-DL.AAC.H.264-GROUP[iT]',
  'Movie.2022.1080p.PLAY.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.WEB-DL.x264-GROUP[PLAY]',
  'Movie.2022.1080p.Hulu.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.DSNY.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.PCOK.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.Peacock.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.PMTP.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.CRiT.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.iP.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.ROKU.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.STAN.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.NOW.WEBRip.x264-GROUP',
  'Movie.2022.1080p.SHO.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.BCORE.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.CRAVE.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.Paramount Plus.WEB-DL.x264-GROUP',
  'Movie.2022.1080p.Apple TV+.WEB-DL.x264-GROUP',
  'Movie.2023.2160p.WEB-DL.HDR10+.DDP5.1.HEVC-GROUP',
  'Movie.2023.2160p.WEB-DL.HDR10+HEVC-GROUP',
  'Movie.2023.2160p.BluRay.HLG.x265-GROUP',
  'Movie.2023.2160p.WEB-DL.DoVi.HEVC-GROUP',
  'Movie.2023.2160p.WEB-DL.Dolby.Vision.HEVC-GROUP',
  'Movie.2023.2160p.WEB-DL.Dolby-Vision.HEVC-GROUP',
  'Movie.2023.2160p.WEB-DL.HDR10.DV.HEVC-GROUP',
  'The.Real.World.S01E01.480p.HDTV.x264-GROUP',
  '2001.A.Space.Odyssey.1968.1080p.BluRay.x264-GROUP',
  'Blade.Runner.2049.2017.2160p.BluRay.x265-GROUP',
  'Movie.2023.1080p.WEB-DL.x264-GROUP[rarbg]',
  'Movie.2023.1080p.WEB-DL.x264',
  'Movie.2023.1080p.WEB.H264-GROUP',
  'Movie.2023.1080p.REPACK.PROPER.BluRay.x264-GROUP',
  'Movie.2023.1080p.BluRay.x264-FLUX',
  'Show.S01E01.1080p.NF.WEB-DL.DDP5.1.x264-NTb',
  'Movie.2023.720p.BluRay.x264-BHDStudio',
  'Movie.2023.2160p.BluRay.x265-BHDStudio',
  'Movie.2023.DVDRip.x264-BMF',
  'Movie.2023.576p.BluRay.x264-BV',
  'Amélie.2001.1080p.BluRay.x264-GROUP',
  'Café2021.1080p.BluRay.x264-GROUP',
  'Show.S1E2.720p.HDTV.x264-GROUP',
  'Show.S100E01.720p.HDTV.x264-GROUP',
]

const streamingInputs = [
  'Movie.2024.NF.WEB-DL',
  'Movie.2024.Netflix.WEB-DL',
  'Movie.2024.Hulu.WEB-DL',
]

const tierInputs: Array<[string, string, string]> = [
  ['hallowed', '1080p', 'bluray'],
  ['FLUX', '1080p', 'web_dl'],
  ['UNKNOWNGROUP', '1080p', 'bluray'],
  ['HALLOWED', '1080p', 'bluray'],
  ['Hallowed', '1080p', 'bluray'],
  ['BHDStudio', '576p', 'any'],
  ['BMF', '1080p', 'dvd'],
]

function writeJson(path: string, value: unknown): void {
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`)
}

const parse = [
  ...testNames.map((name) => ({ origin: 'test', name, expected: parseReleaseName(name) })),
  ...extraNames.map((name) => ({ origin: 'extra', name, expected: parseReleaseName(name) })),
]
writeJson(join(here, 'parse.json'), parse)

writeJson(
  join(here, 'streaming-services.json'),
  streamingInputs.map((name) => ({ name, expected: detectStreamingService(name) ?? null }))
)

writeJson(
  join(here, 'group-tiers.json'),
  tierInputs.map(([group, resolution, source]) => ({
    group,
    resolution,
    source,
    expected: getReleaseGroupTier(group, resolution, source).tier,
  }))
)

// Exact duplicates are dropped; lookups take the first matching row, so order is kept.
const rows: string[] = []
for (const [key, entries] of Object.entries(releaseGroupTiers)) {
  const seen = new Set<string>()
  for (const entry of entries) {
    const row = [key, entry.group, entry.tier, entry.resolution, entry.source].join('\t')
    if (!seen.has(row)) {
      seen.add(row)
      rows.push(row)
    }
  }
}
const header = [
  '# Release group tiers from the Dictionarry database (https://github.com/Dictionarry-Hub/database),',
  '# exported from packages/parser by fixtures/release-names/export.ts.',
  '# key\tgroup\ttier\tresolution\tsource',
]
writeFileSync(join(crateData, 'release-groups.tsv'), `${[...header, ...rows].join('\n')}\n`)

console.log(
  `parse: ${parse.length}, streaming: ${streamingInputs.length}, tiers: ${tierInputs.length}`
)

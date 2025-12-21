/**
 * Dictionarry Data Generator
 *
 * Fetches regex patterns and custom formats from the Dictionarry database
 * and generates TypeScript files for use in the parser.
 *
 * Data source: https://github.com/Dictionarry-Hub/database
 */

import { parse as parseYaml } from 'yaml'
import { writeFileSync, mkdirSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const DATA_DIR = join(__dirname, '../src/data')

const GITHUB_API = 'https://api.github.com/repos/Dictionarry-Hub/database/contents'
const RAW_URL = 'https://raw.githubusercontent.com/Dictionarry-Hub/database/stable'
const BRANCH = 'stable'

interface GitHubFile {
  name: string
  download_url: string
}

interface DictionarryPattern {
  name: string
  pattern: string
  description?: string
  tags?: string[]
}

interface DictionarryCondition {
  name: string
  type: 'resolution' | 'source' | 'release_title' | 'release_group'
  required: boolean
  negate?: boolean
  resolution?: string
  source?: string
  pattern?: string
}

interface DictionarryCustomFormat {
  name: string
  description?: string
  tags?: string[]
  conditions: DictionarryCondition[]
}

interface StreamingService {
  name: string
  abbreviation: string
  pattern: string
}

interface ReleaseGroupTier {
  group: string
  tier: 1 | 2 | 3
  resolution: string
  source: string
}

async function fetchJson<T>(url: string): Promise<T> {
  const response = await fetch(url)
  if (!response.ok) {
    throw new Error(`Failed to fetch ${url}: ${response.status}`)
  }
  return response.json()
}

async function fetchText(url: string): Promise<string> {
  const response = await fetch(url)
  if (!response.ok) {
    throw new Error(`Failed to fetch ${url}: ${response.status}`)
  }
  return response.text()
}

async function fetchYamlFiles(directory: string): Promise<Map<string, DictionarryPattern | DictionarryCustomFormat>> {
  const files = await fetchJson<GitHubFile[]>(`${GITHUB_API}/${directory}?ref=${BRANCH}`)
  const results = new Map()

  const yamlFiles = files.filter((f) => f.name.endsWith('.yml') && f.name !== '.gitkeep')

  console.log(`Fetching ${yamlFiles.length} files from ${directory}...`)

  // Fetch in batches to avoid rate limiting
  const batchSize = 20
  for (let i = 0; i < yamlFiles.length; i += batchSize) {
    const batch = yamlFiles.slice(i, i + batchSize)
    const contents = await Promise.all(
      batch.map(async (file) => {
        const content = await fetchText(file.download_url)
        return { name: file.name, content }
      })
    )

    for (const { name, content } of contents) {
      try {
        const parsed = parseYaml(content)
        results.set(name, parsed)
      } catch (e) {
        console.warn(`Failed to parse ${name}:`, e)
      }
    }

    if (i + batchSize < yamlFiles.length) {
      // Small delay to avoid rate limiting
      await new Promise((r) => setTimeout(r, 100))
    }
  }

  return results
}

function extractStreamingServices(patterns: Map<string, DictionarryPattern>): StreamingService[] {
  const services: StreamingService[] = []

  for (const [, pattern] of patterns) {
    if (!pattern.tags?.includes('Streaming Service')) continue

    // Extract abbreviation from pattern name or pattern itself
    const abbrevMatch = pattern.pattern.match(/\\b\(([A-Z]+)\|/i) || pattern.pattern.match(/\\b([A-Z]{2,5})\\b/i)

    services.push({
      name: pattern.name,
      abbreviation: abbrevMatch?.[1]?.toUpperCase() || pattern.name.toUpperCase().replace(/[^A-Z]/g, '').slice(0, 4),
      pattern: pattern.pattern,
    })
  }

  return services
}

function extractReleaseGroupTiers(customFormats: Map<string, DictionarryCustomFormat>): ReleaseGroupTier[] {
  const tiers: ReleaseGroupTier[] = []

  for (const [, format] of customFormats) {
    if (!format.tags?.includes('Release Group Tier')) continue

    // Determine tier from format name
    let tier: 1 | 2 | 3 = 3
    if (format.name.includes('Tier 1')) tier = 1
    else if (format.name.includes('Tier 2')) tier = 2
    else if (format.name.includes('Tier 3')) tier = 3

    // Extract resolution and source from conditions
    let resolution = 'any'
    let source = 'any'

    for (const condition of format.conditions) {
      if (condition.type === 'resolution' && condition.required) {
        resolution = condition.resolution || 'any'
      }
      if (condition.type === 'source' && condition.required) {
        source = condition.source || 'any'
      }
    }

    // Extract release groups from conditions
    for (const condition of format.conditions) {
      if (condition.type === 'release_group' && condition.pattern) {
        tiers.push({
          group: condition.pattern,
          tier,
          resolution,
          source,
        })
      }
    }
  }

  return tiers
}

function generateStreamingServicesFile(services: StreamingService[]): string {
  const sortedServices = services.sort((a, b) => a.name.localeCompare(b.name))

  return `// Auto-generated from Dictionarry database
// Source: https://github.com/Dictionarry-Hub/database
// Generated: ${new Date().toISOString()}

export interface StreamingService {
  name: string
  abbreviation: string
  pattern: RegExp
}

export const streamingServices: StreamingService[] = [
${sortedServices.map((s) => `  { name: '${s.name}', abbreviation: '${s.abbreviation}', pattern: /${s.pattern}/i },`).join('\n')}
]

export function detectStreamingService(name: string): string | undefined {
  for (const service of streamingServices) {
    if (service.pattern.test(name)) {
      return service.abbreviation
    }
  }
  return undefined
}
`
}

function generateReleaseGroupsFile(tiers: ReleaseGroupTier[]): string {
  // Group by release group name (lowercase for case-insensitive lookup)
  const grouped = new Map<string, ReleaseGroupTier[]>()

  for (const tier of tiers) {
    const key = tier.group.toLowerCase()
    if (!grouped.has(key)) {
      grouped.set(key, [])
    }
    grouped.get(key)!.push(tier)
  }

  // Sort entries for consistent output
  const sortedEntries = Array.from(grouped.entries()).sort((a, b) => a[0].localeCompare(b[0]))

  return `// Auto-generated from Dictionarry database
// Source: https://github.com/Dictionarry-Hub/database
// Generated: ${new Date().toISOString()}

export interface ReleaseGroupTier {
  group: string
  tier: 1 | 2 | 3
  resolution: string
  source: string
}

export const releaseGroupTiers: Record<string, ReleaseGroupTier[]> = {
${sortedEntries.map(([key, entries]) => `  '${key}': ${JSON.stringify(entries)},`).join('\n')}
}

export interface ReleaseGroupInfo {
  name: string
  tier: 1 | 2 | 3 | null
}

export function getReleaseGroupTier(
  groupName: string,
  resolution: string | undefined,
  source: string | undefined
): ReleaseGroupInfo {
  const key = groupName.toLowerCase()
  const entries = releaseGroupTiers[key]

  if (!entries || entries.length === 0) {
    return { name: groupName, tier: null }
  }

  // Find best matching entry
  const match = entries.find(
    (e) =>
      (e.resolution === 'any' || e.resolution === resolution) &&
      (e.source === 'any' || e.source === source)
  ) || entries[0]

  return { name: groupName, tier: match.tier }
}
`
}

async function main() {
  console.log('Fetching Dictionarry data...')

  // Ensure data directory exists
  mkdirSync(DATA_DIR, { recursive: true })

  // Fetch regex patterns
  const patterns = (await fetchYamlFiles('regex_patterns')) as Map<string, DictionarryPattern>
  console.log(`Loaded ${patterns.size} regex patterns`)

  // Fetch custom formats
  const customFormats = (await fetchYamlFiles('custom_formats')) as Map<string, DictionarryCustomFormat>
  console.log(`Loaded ${customFormats.size} custom formats`)

  // Extract and generate streaming services
  const streamingServices = extractStreamingServices(patterns)
  console.log(`Found ${streamingServices.length} streaming services`)

  const streamingServicesContent = generateStreamingServicesFile(streamingServices)
  writeFileSync(join(DATA_DIR, 'streaming-services.ts'), streamingServicesContent)
  console.log('Generated streaming-services.ts')

  // Extract and generate release group tiers
  const releaseGroupTiers = extractReleaseGroupTiers(customFormats)
  console.log(`Found ${releaseGroupTiers.length} release group tier mappings`)

  const releaseGroupsContent = generateReleaseGroupsFile(releaseGroupTiers)
  writeFileSync(join(DATA_DIR, 'release-groups.ts'), releaseGroupsContent)
  console.log('Generated release-groups.ts')

  console.log('Done!')
}

main().catch(console.error)

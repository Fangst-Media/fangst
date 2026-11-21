import type { IndexerRegistryEntry } from '../types/indexer'

/**
 * Registry of known indexers mapped by their domain.
 * When adding an indexer, users can select from this list and only need to provide their API key.
 */
export const INDEXER_REGISTRY: Record<string, IndexerRegistryEntry> = {
  // Popular Newznab indexers
  'api.nzbgeek.info': {
    name: 'NZBgeek',
    protocol: 'newznab',
    defaultUrl: 'https://api.nzbgeek.info',
    categories: {
      movies: [2000, 2010, 2020, 2030, 2040, 2045, 2050, 2060],
      tv: [5000, 5020, 5030, 5040, 5045, 5050, 5060, 5070, 5080],
    },
  },
  'drunkenslug.com': {
    name: 'DrunkenSlug',
    protocol: 'newznab',
    defaultUrl: 'https://api.drunkenslug.com',
  },
  'nzb.su': {
    name: 'NZB.su',
    protocol: 'newznab',
    defaultUrl: 'https://api.nzb.su',
  },
  'althub.co.za': {
    name: 'altHUB',
    protocol: 'newznab',
    defaultUrl: 'https://api.althub.co.za',
  },
  'ninjacentral.co.za': {
    name: 'NinjaCentral',
    protocol: 'newznab',
    defaultUrl: 'https://ninjacentral.co.za',
    categories: {
      movies: [2000, 2010, 2020, 2030, 2040, 2050, 2060],
      tv: [5000, 5020, 5030, 5040, 5050, 5060, 5070, 5080],
    },
  },
  'nzbfinder.ws': {
    name: 'NZBFinder',
    protocol: 'newznab',
    defaultUrl: 'https://nzbfinder.ws',
  },
  'nzbplanet.net': {
    name: 'NZBPlanet',
    protocol: 'newznab',
    defaultUrl: 'https://api.nzbplanet.net',
  },
  'oznzb.com': {
    name: 'OZnzb',
    protocol: 'newznab',
    defaultUrl: 'https://api.oznzb.com',
  },
  'tabula-rasa.pw': {
    name: 'Tabula Rasa',
    protocol: 'newznab',
    defaultUrl: 'https://tabula-rasa.pw',
  },
  'nzbcat.com': {
    name: 'NZBCat',
    protocol: 'newznab',
    defaultUrl: 'https://nzbcat.com',
  },
  'usenet.farm': {
    name: 'Usenet.Farm',
    protocol: 'newznab',
    defaultUrl: 'https://usenet.farm',
  },
  'simplynzbs.com': {
    name: 'SimplyNZBs',
    protocol: 'newznab',
    defaultUrl: 'https://simplynzbs.com',
  },
  'nzbnoob.com': {
    name: 'NZBnoob',
    protocol: 'newznab',
    defaultUrl: 'https://nzbnoob.com',
  },
  'abnzb.com': {
    name: 'ABnzb',
    protocol: 'newznab',
    defaultUrl: 'https://abnzb.com',
  },
  'nzbsu.com': {
    name: 'nzbsu',
    protocol: 'newznab',
    defaultUrl: 'https://api.nzbsu.com',
  },
  'dog.nzb.cr': {
    name: 'DOGnzb',
    protocol: 'newznab',
    defaultUrl: 'https://api.dognzb.cr',
  },
}

/**
 * Get a list of all known indexers for display in the UI
 */
export function getIndexerList(): Array<{ domain: string; name: string; protocol: string }> {
  return Object.entries(INDEXER_REGISTRY)
    .map(([domain, entry]) => ({
      domain,
      name: entry.name,
      protocol: entry.protocol,
    }))
    .sort((a, b) => a.name.localeCompare(b.name))
}

/**
 * Find an indexer entry by domain
 */
export function findIndexerByDomain(domain: string): IndexerRegistryEntry | undefined {
  const normalized = domain
    .toLowerCase()
    .replace(/^(https?:\/\/)?(www\.)?/, '')
    .replace(/\/$/, '')
  return INDEXER_REGISTRY[normalized]
}

/**
 * Extract domain from a URL
 */
export function extractDomain(url: string): string {
  try {
    const parsed = new URL(url)
    return parsed.hostname.replace(/^www\./, '')
  } catch {
    return (
      url
        .toLowerCase()
        .replace(/^(https?:\/\/)?(www\.)?/, '')
        .replace(/\/$/, '')
        .split('/')[0] ?? url
    )
  }
}

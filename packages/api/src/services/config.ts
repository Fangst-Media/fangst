import { readFileSync, writeFileSync, existsSync, mkdirSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import YAML from 'yaml'
import {
  settingsSchema,
  downloaderSchema,
  type Settings,
  type LockedSettings,
  type SettingsUpdate,
  type Downloader,
  type CreateDownloader,
  type UpdateDownloader,
} from '@jaeger/shared/types/settings'
import {
  indexerSchema,
  type Indexer,
  type CreateIndexer,
  type UpdateIndexer,
} from '@jaeger/shared/types/indexer'

// ENV variable to settings path mapping
const ENV_MAPPING: Record<string, string> = {
  PORT: 'server.port',
  CORS_ORIGIN: 'server.corsOrigin',
  DATABASE_URL: 'database.url',
  JAEGER_API_KEY: 'security.apiKey',
  SESSION_EXPIRY_DAYS: 'security.sessionExpiryDays',
  JELLYFIN_URL: 'auth.jellyfin.url',
  SYNC_ADMIN_FROM_PROVIDER: 'auth.jellyfin.syncAdminFromProvider',
  MOCK_AUTH: 'auth.mock.enabled',
  MEDIA_PATH: 'media.path',
  MOCK_FILESYSTEM: 'media.mockFilesystem',
  METADATA_PROVIDERS: 'metadata.providers',
  METADATA_RELAY_URL: 'metadata.relay.url',
  TMDB_API_KEY: 'metadata.tmdb.apiKey',
  TVDB_API_KEY: 'metadata.tvdb.apiKey',
}

type YamlConfig = Partial<Settings> & { downloaders?: Downloader[]; indexers?: Indexer[] }

class ConfigService {
  private settings: Settings | null = null
  private locked: LockedSettings = {}
  private downloaders: Downloader[] = []
  private indexers: Indexer[] = []
  private yamlPath: string
  private initialized = false

  constructor() {
    this.yamlPath = resolve(process.cwd(), './data/settings.yaml')
  }

  async initialize(): Promise<void> {
    if (this.initialized) return

    // Ensure data directory exists
    const dataDir = dirname(this.yamlPath)
    if (!existsSync(dataDir)) {
      mkdirSync(dataDir, { recursive: true })
    }

    // Load settings with priority: ENV > YAML > defaults
    const yamlConfig = this.loadYamlConfig()
    const { downloaders: yamlDownloaders, indexers: yamlIndexers, ...yamlSettings } = yamlConfig
    const envOverrides = this.getEnvOverrides()

    // Merge: start with YAML (or empty), apply ENV overrides
    const merged = this.deepMerge(yamlSettings, envOverrides) as Record<string, unknown>

    // Handle API key generation
    if (!merged.security || !(merged.security as Record<string, unknown>).apiKey) {
      merged.security = (merged.security as Record<string, unknown>) || {}
      ;(merged.security as Record<string, unknown>).apiKey = this.generateApiKey()
      // Save immediately so API key persists
      const yamlWithKey = this.deepMerge(yamlSettings, {
        security: { apiKey: (merged.security as Record<string, unknown>).apiKey },
      })
      this.saveToYaml({ ...yamlWithKey, downloaders: yamlDownloaders, indexers: yamlIndexers } as YamlConfig)
    }

    // Validate and apply defaults
    this.settings = settingsSchema.parse(merged)

    // Load and validate downloaders
    this.downloaders = (yamlDownloaders || []).map((d) => downloaderSchema.parse(d))

    // Load and validate indexers
    this.indexers = (yamlIndexers || []).map((i) => indexerSchema.parse(i))

    this.initialized = true

    console.log(`[Config] Loaded settings from ${this.yamlPath}`)
    console.log(`[Config] Locked settings: ${Object.keys(this.locked).length > 0 ? JSON.stringify(this.locked) : 'none'}`)
    console.log(`[Config] Loaded ${this.downloaders.length} downloader(s)`)
    console.log(`[Config] Loaded ${this.indexers.length} indexer(s)`)
  }

  private loadYamlConfig(): YamlConfig {
    if (!existsSync(this.yamlPath)) {
      return {}
    }

    try {
      const content = readFileSync(this.yamlPath, 'utf-8')
      return YAML.parse(content) || {}
    } catch (error) {
      console.warn(`[Config] Failed to parse settings.yaml: ${error}`)
      return {}
    }
  }

  private getEnvOverrides(): Partial<Settings> {
    const overrides: Record<string, unknown> = {}

    for (const [envKey, settingPath] of Object.entries(ENV_MAPPING)) {
      const envValue = process.env[envKey]
      if (envValue === undefined) continue

      // Mark as locked
      this.setLockedPath(settingPath, true)

      // Parse and set value
      const parsedValue = this.parseEnvValue(envKey, envValue)
      this.setNestedValue(overrides, settingPath, parsedValue)
    }

    return overrides as Partial<Settings>
  }

  private parseEnvValue(key: string, value: string): unknown {
    // Boolean conversions
    if (['MOCK_FILESYSTEM', 'MOCK_AUTH', 'SYNC_ADMIN_FROM_PROVIDER'].includes(key)) {
      return value === 'true'
    }
    // Number conversions
    if (['PORT', 'SESSION_EXPIRY_DAYS'].includes(key)) {
      return Number.parseInt(value, 10)
    }
    // Array conversions
    if (key === 'METADATA_PROVIDERS') {
      return value.split(',').map((s) => s.trim())
    }
    return value
  }

  private setNestedValue(obj: Record<string, unknown>, path: string, value: unknown): void {
    const keys = path.split('.')
    let current = obj

    for (let i = 0; i < keys.length - 1; i++) {
      const key = keys[i]!
      if (!(key in current)) {
        current[key] = {}
      }
      current = current[key] as Record<string, unknown>
    }

    const lastKey = keys[keys.length - 1]!
    current[lastKey] = value
  }

  private setLockedPath(path: string, locked: boolean): void {
    const keys = path.split('.')
    if (keys.length < 2) return

    const category = keys[0] as keyof Settings
    const setting = keys.slice(1).join('.')

    if (!this.locked[category]) {
      this.locked[category] = {}
    }
    ;(this.locked[category] as Record<string, boolean>)[setting] = locked
  }

  private deepMerge(
    target: Record<string, unknown>,
    source: Record<string, unknown>
  ): Record<string, unknown> {
    const result = { ...target }

    for (const key of Object.keys(source)) {
      if (source[key] && typeof source[key] === 'object' && !Array.isArray(source[key])) {
        result[key] = this.deepMerge(
          (result[key] as Record<string, unknown>) || {},
          source[key] as Record<string, unknown>
        )
      } else {
        result[key] = source[key]
      }
    }

    return result
  }

  private generateApiKey(): string {
    return crypto.randomUUID() + crypto.randomUUID().replace(/-/g, '')
  }

  private saveToYaml(config: YamlConfig): void {
    const content = YAML.stringify(config, { indent: 2 })
    writeFileSync(this.yamlPath, content, 'utf-8')
  }

  get(): Settings {
    if (!this.settings) {
      throw new Error('ConfigService not initialized. Call initialize() first.')
    }
    return this.settings
  }

  getLocked(): LockedSettings {
    return this.locked
  }

  isLocked(path: string): boolean {
    const keys = path.split('.')
    if (keys.length < 2) return false

    const category = keys[0] as keyof Settings
    const setting = keys.slice(1).join('.')

    return !!(this.locked[category] as Record<string, boolean> | undefined)?.[setting]
  }

  async update(updates: SettingsUpdate): Promise<Settings> {
    if (!this.settings) {
      throw new Error('ConfigService not initialized')
    }

    // Filter out locked settings
    const filteredUpdates = this.filterLockedUpdates(updates)

    // Load current YAML (preserving downloaders and indexers)
    const yamlConfig = this.loadYamlConfig()
    const { downloaders, indexers, ...yamlSettings } = yamlConfig

    // Merge updates into YAML settings
    const newYamlSettings = this.deepMerge(
      yamlSettings as Record<string, unknown>,
      filteredUpdates as Record<string, unknown>
    ) as Partial<Settings>

    // Save to YAML (preserving downloaders and indexers)
    this.saveToYaml({ ...newYamlSettings, downloaders, indexers })

    // Reload with ENV overrides
    const envOverrides = this.getEnvOverrides()
    const merged = this.deepMerge(
      newYamlSettings as Record<string, unknown>,
      envOverrides as Record<string, unknown>
    )

    this.settings = settingsSchema.parse(merged)
    return this.settings
  }

  private filterLockedUpdates(updates: SettingsUpdate): SettingsUpdate {
    const filtered: SettingsUpdate = {}

    for (const [category, values] of Object.entries(updates)) {
      if (!values) continue

      const filteredValues: Record<string, unknown> = {}
      for (const [key, value] of Object.entries(values as Record<string, unknown>)) {
        const path = `${category}.${key}`
        if (!this.isLocked(path)) {
          filteredValues[key] = value
        }
      }

      if (Object.keys(filteredValues).length > 0) {
        ;(filtered as Record<string, unknown>)[category] = filteredValues
      }
    }

    return filtered
  }

  async regenerateApiKey(): Promise<string> {
    if (this.isLocked('security.apiKey')) {
      throw new Error('API key is locked by environment variable')
    }

    const newKey = this.generateApiKey()

    const yamlConfig = this.loadYamlConfig()
    const { downloaders, indexers, ...yamlSettings } = yamlConfig
    yamlSettings.security = yamlSettings.security || { apiKey: '', sessionExpiryDays: 30 }
    yamlSettings.security.apiKey = newKey
    this.saveToYaml({ ...yamlSettings, downloaders, indexers })

    if (this.settings) {
      this.settings.security.apiKey = newKey
    }

    return newKey
  }

  // Convenience getters for common settings
  getPort(): number {
    return this.get().server.port
  }

  getCorsOrigin(): string {
    return this.get().server.corsOrigin
  }

  getDatabaseUrl(): string {
    return this.get().database.url
  }

  getMediaPath(): string {
    return this.get().media.path
  }

  isMockFilesystem(): boolean {
    return this.get().media.mockFilesystem
  }

  isMockAuth(): boolean {
    return this.get().auth.mock.enabled
  }

  getJellyfinUrl(): string {
    return this.get().auth.jellyfin.url
  }

  getSyncAdminFromProvider(): boolean {
    return this.get().auth.jellyfin.syncAdminFromProvider
  }

  getSessionExpiryDays(): number {
    return this.get().security.sessionExpiryDays
  }

  getMetadataProviders(): Array<'relay' | 'tmdb' | 'tvdb'> {
    return this.get().metadata.providers
  }

  getMetadataRelayUrl(): string {
    return this.get().metadata.relay.url
  }

  getTmdbApiKey(): string {
    return this.get().metadata.tmdb.apiKey
  }

  getTvdbApiKey(): string {
    return this.get().metadata.tvdb.apiKey
  }

  getApiKey(): string {
    return this.get().security.apiKey
  }

  // Downloader CRUD methods
  getDownloaders(): Downloader[] {
    return this.downloaders
  }

  getDownloader(id: string): Downloader | undefined {
    return this.downloaders.find((d) => d.id === id)
  }

  getEnabledDownloaders(): Downloader[] {
    return this.downloaders.filter((d) => d.enabled)
  }

  addDownloader(data: CreateDownloader): Downloader {
    const downloader: Downloader = {
      ...data,
      id: crypto.randomUUID(),
      enabled: data.enabled ?? true,
    }

    this.downloaders.push(downloader)
    this.saveDownloaders()

    return downloader
  }

  updateDownloader(id: string, updates: UpdateDownloader): Downloader {
    const index = this.downloaders.findIndex((d) => d.id === id)
    if (index === -1) {
      throw new Error(`Downloader with id ${id} not found`)
    }

    const existing = this.downloaders[index]!
    const updated: Downloader = { ...existing, ...updates }
    this.downloaders[index] = updated
    this.saveDownloaders()

    return updated
  }

  deleteDownloader(id: string): void {
    const index = this.downloaders.findIndex((d) => d.id === id)
    if (index === -1) {
      throw new Error(`Downloader with id ${id} not found`)
    }

    this.downloaders.splice(index, 1)
    this.saveDownloaders()
  }

  private saveDownloaders(): void {
    const yamlConfig = this.loadYamlConfig()
    const { downloaders: _, indexers, ...yamlSettings } = yamlConfig
    this.saveToYaml({ ...yamlSettings, downloaders: this.downloaders, indexers })
  }

  // Indexer CRUD methods
  getIndexers(): Indexer[] {
    return this.indexers
  }

  getEnabledIndexers(): Indexer[] {
    return this.indexers.filter((i) => i.enabled)
  }

  getIndexer(id: string): Indexer | undefined {
    return this.indexers.find((i) => i.id === id)
  }

  addIndexer(data: CreateIndexer): Indexer {
    const indexer: Indexer = {
      ...data,
      id: crypto.randomUUID(),
      enabled: data.enabled ?? true,
    }

    this.indexers.push(indexer)
    this.saveIndexers()

    return indexer
  }

  updateIndexer(id: string, updates: UpdateIndexer): Indexer {
    const index = this.indexers.findIndex((i) => i.id === id)
    if (index === -1) {
      throw new Error(`Indexer with id ${id} not found`)
    }

    const existing = this.indexers[index]!
    const updated: Indexer = { ...existing, ...updates }
    this.indexers[index] = updated
    this.saveIndexers()

    return updated
  }

  deleteIndexer(id: string): void {
    const index = this.indexers.findIndex((i) => i.id === id)
    if (index === -1) {
      throw new Error(`Indexer with id ${id} not found`)
    }

    this.indexers.splice(index, 1)
    this.saveIndexers()
  }

  private saveIndexers(): void {
    const yamlConfig = this.loadYamlConfig()
    const { indexers: _, downloaders, ...yamlSettings } = yamlConfig
    this.saveToYaml({ ...yamlSettings, downloaders, indexers: this.indexers })
  }
}

export const configService = new ConfigService()

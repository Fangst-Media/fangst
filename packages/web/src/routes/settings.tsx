import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import type { SettingsResponse, SettingsUpdate } from '@jaeger/shared/types/settings'

const API_BASE_URL = 'http://localhost:5001'

async function fetchSettings(): Promise<SettingsResponse> {
  const response = await fetch(`${API_BASE_URL}/api/settings`, {
    credentials: 'include',
  })
  if (!response.ok) throw new Error('Failed to fetch settings')
  return response.json()
}

async function updateSettings(updates: SettingsUpdate): Promise<SettingsResponse> {
  const response = await fetch(`${API_BASE_URL}/api/settings`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'include',
    body: JSON.stringify(updates),
  })
  if (!response.ok) throw new Error('Failed to update settings')
  return response.json()
}

async function regenerateApiKey(): Promise<{ apiKey: string }> {
  const response = await fetch(`${API_BASE_URL}/api/settings/api-key/regenerate`, {
    method: 'POST',
    credentials: 'include',
  })
  if (!response.ok) throw new Error('Failed to regenerate API key')
  return response.json()
}

async function getApiKey(): Promise<{ apiKey: string; locked: boolean }> {
  const response = await fetch(`${API_BASE_URL}/api/settings/api-key`, {
    credentials: 'include',
  })
  if (!response.ok) throw new Error('Failed to get API key')
  return response.json()
}

function LockedIndicator({ locked }: { locked: boolean }) {
  if (!locked) return null
  return (
    <span
      className="ml-2 px-1.5 py-0.5 text-[10px] bg-yellow-500/20 text-yellow-400 rounded font-medium"
      title="Set via environment variable"
    >
      ENV
    </span>
  )
}

type SettingFieldProps = {
  label: string
  description?: string
  locked?: boolean
  children: React.ReactNode
}

function SettingField({ label, description, locked, children }: SettingFieldProps) {
  return (
    <div className="py-4 border-b border-border last:border-0">
      <div className="flex items-start justify-between gap-4">
        <div className="flex-1">
          <label className="font-medium text-foreground flex items-center">
            {label}
            <LockedIndicator locked={locked ?? false} />
          </label>
          {description && <p className="text-sm text-muted mt-1">{description}</p>}
        </div>
        <div className="shrink-0">{children}</div>
      </div>
    </div>
  )
}

type SettingsSectionProps = {
  title: string
  description?: string
  children: React.ReactNode
}

function SettingsSection({ title, description, children }: SettingsSectionProps) {
  return (
    <div className="rounded-lg bg-card border border-border p-6 mb-6">
      <h2 className="text-lg font-semibold text-foreground mb-1">{title}</h2>
      {description && <p className="text-sm text-muted mb-4">{description}</p>}
      <div>{children}</div>
    </div>
  )
}

export default function Settings() {
  const queryClient = useQueryClient()
  const [showApiKey, setShowApiKey] = useState(false)
  const [copiedKey, setCopiedKey] = useState(false)

  const { data, isLoading, error } = useQuery({
    queryKey: ['settings'],
    queryFn: fetchSettings,
  })

  const { data: apiKeyData } = useQuery({
    queryKey: ['apiKey'],
    queryFn: getApiKey,
    enabled: showApiKey,
  })

  const updateMutation = useMutation({
    mutationFn: updateSettings,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings'] })
    },
  })

  const regenerateMutation = useMutation({
    mutationFn: regenerateApiKey,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['settings'] })
      queryClient.invalidateQueries({ queryKey: ['apiKey'] })
    },
  })

  const handleCopyApiKey = async () => {
    if (apiKeyData?.apiKey) {
      await navigator.clipboard.writeText(apiKeyData.apiKey)
      setCopiedKey(true)
      setTimeout(() => setCopiedKey(false), 2000)
    }
  }

  if (isLoading) {
    return (
      <div className="min-h-screen p-6 flex items-center justify-center">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
      </div>
    )
  }

  if (error || !data) {
    return (
      <div className="min-h-screen p-6">
        <div className="rounded-lg bg-card border border-red-500/50 p-6 text-red-400">
          Failed to load settings: {error instanceof Error ? error.message : 'Unknown error'}
        </div>
      </div>
    )
  }

  const { settings, locked } = data

  const isLocked = (path: string): boolean => {
    const [category, ...rest] = path.split('.')
    const key = rest.join('.')
    return !!(locked as Record<string, Record<string, boolean>>)?.[category]?.[key]
  }

  return (
    <div className="min-h-screen p-6">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-foreground">Settings</h1>
        <p className="text-sm text-muted">Configure your Jaeger instance</p>
      </div>

      <div className="max-w-3xl">
        {/* Appearance */}
        <SettingsSection title="Appearance" description="Customize the look and feel">
          <SettingField label="Theme" locked={isLocked('ui.theme')}>
            <select
              className="rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm min-w-[120px]"
              value={settings.ui.theme}
              disabled={isLocked('ui.theme')}
              onChange={(e) =>
                updateMutation.mutate({ ui: { theme: e.target.value as 'dark' | 'light' | 'system' } })
              }
            >
              <option value="system">System</option>
              <option value="dark">Dark</option>
              <option value="light">Light</option>
            </select>
          </SettingField>

          <SettingField label="Language" locked={isLocked('ui.language')}>
            <select
              className="rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm min-w-[120px]"
              value={settings.ui.language}
              disabled={isLocked('ui.language')}
              onChange={(e) => updateMutation.mutate({ ui: { language: e.target.value } })}
            >
              <option value="en">English</option>
              <option value="da">Dansk</option>
              <option value="de">Deutsch</option>
            </select>
          </SettingField>

          <SettingField label="Default Library View" locked={isLocked('ui.defaultLibraryView')}>
            <select
              className="rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm min-w-[120px]"
              value={settings.ui.defaultLibraryView}
              disabled={isLocked('ui.defaultLibraryView')}
              onChange={(e) =>
                updateMutation.mutate({
                  ui: { defaultLibraryView: e.target.value as 'grid' | 'list' | 'table' },
                })
              }
            >
              <option value="grid">Grid</option>
              <option value="list">List</option>
              <option value="table">Table</option>
            </select>
          </SettingField>
        </SettingsSection>

        {/* Security */}
        <SettingsSection title="Security" description="API access and authentication">
          <SettingField
            label="API Key"
            description="Used for external integrations and API access"
            locked={isLocked('security.apiKey')}
          >
            <div className="flex items-center gap-2">
              <code className="px-2 py-1 bg-background border border-border rounded text-xs font-mono text-muted">
                {showApiKey && apiKeyData ? apiKeyData.apiKey : settings.security.apiKey}
              </code>
              <button
                type="button"
                onClick={() => setShowApiKey(!showApiKey)}
                className="px-2 py-1 text-xs rounded border border-border text-muted hover:text-foreground hover:bg-background transition-colors"
              >
                {showApiKey ? 'Hide' : 'Show'}
              </button>
              {showApiKey && (
                <button
                  type="button"
                  onClick={handleCopyApiKey}
                  className="px-2 py-1 text-xs rounded border border-border text-muted hover:text-foreground hover:bg-background transition-colors"
                >
                  {copiedKey ? 'Copied!' : 'Copy'}
                </button>
              )}
              {!isLocked('security.apiKey') && (
                <button
                  type="button"
                  onClick={() => regenerateMutation.mutate()}
                  disabled={regenerateMutation.isPending}
                  className="px-2 py-1 text-xs rounded bg-red-500/20 text-red-400 hover:bg-red-500/30 transition-colors disabled:opacity-50"
                >
                  Regenerate
                </button>
              )}
            </div>
          </SettingField>

          <SettingField
            label="Session Expiry"
            description="How long user sessions remain valid"
            locked={isLocked('security.sessionExpiryDays')}
          >
            <div className="flex items-center gap-2">
              <input
                type="number"
                min="1"
                max="365"
                className="w-20 rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm"
                value={settings.security.sessionExpiryDays}
                disabled={isLocked('security.sessionExpiryDays')}
                onChange={(e) =>
                  updateMutation.mutate({
                    security: { sessionExpiryDays: Number.parseInt(e.target.value, 10) },
                  })
                }
              />
              <span className="text-sm text-muted">days</span>
            </div>
          </SettingField>
        </SettingsSection>

        {/* Media Library */}
        <SettingsSection title="Media Library" description="Configure your media library path">
          <SettingField
            label="Media Path"
            description="Root directory for your media files"
            locked={isLocked('media.path')}
          >
            <input
              type="text"
              className="w-64 rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm font-mono"
              value={settings.media.path}
              disabled={isLocked('media.path')}
              onChange={(e) => updateMutation.mutate({ media: { path: e.target.value } })}
            />
          </SettingField>
        </SettingsSection>

        {/* Request Permissions */}
        <SettingsSection title="Request Permissions" description="Configure how different roles can make requests">
          <SettingField
            label="Member Request Mode"
            description="How members interact with the request system"
          >
            <select
              className="rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm min-w-[150px]"
              value={settings.requests?.memberRequestMode ?? 'simple_only'}
              onChange={(e) =>
                updateMutation.mutate({
                  requests: { memberRequestMode: e.target.value as 'simple_only' | 'detailed_only' | 'user_choice' },
                })
              }
            >
              <option value="simple_only">Simple Mode Only</option>
              <option value="detailed_only">Detailed Mode Only</option>
              <option value="user_choice">User Choice</option>
            </select>
          </SettingField>

          <SettingField
            label="Admin Request Mode"
            description="How admins interact with the request system"
          >
            <select
              className="rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm min-w-[150px]"
              value={settings.requests?.adminRequestMode ?? 'detailed_only'}
              onChange={(e) =>
                updateMutation.mutate({
                  requests: { adminRequestMode: e.target.value as 'simple_only' | 'detailed_only' | 'user_choice' },
                })
              }
            >
              <option value="simple_only">Simple Mode Only</option>
              <option value="detailed_only">Detailed Mode Only</option>
              <option value="user_choice">User Choice</option>
            </select>
          </SettingField>

          <SettingField
            label="Default Simple Quality"
            description="Default quality preset for simple mode"
          >
            <select
              className="rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm min-w-[120px]"
              value={settings.requests?.defaultSimpleQuality ?? '1080p'}
              onChange={(e) =>
                updateMutation.mutate({
                  requests: { defaultSimpleQuality: e.target.value as '2160p' | '1080p' | '720p' | 'any' },
                })
              }
            >
              <option value="2160p">4K (2160p)</option>
              <option value="1080p">1080p</option>
              <option value="720p">720p</option>
              <option value="any">Any</option>
            </select>
          </SettingField>
        </SettingsSection>

        {/* Size Filter */}
        <SettingsSection
          title="Size Filter"
          description="Filter releases based on file size per minute of runtime (TRaSH Guides recommendations)"
        >
          <SettingField
            label="Enable Size Filter"
            description="Reject releases with suspiciously small file sizes"
          >
            <button
              type="button"
              onClick={() =>
                updateMutation.mutate({
                  sizeFilter: { enabled: !settings.sizeFilter?.enabled },
                })
              }
              className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                settings.sizeFilter?.enabled ? 'bg-accent' : 'bg-border'
              }`}
            >
              <span
                className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                  settings.sizeFilter?.enabled ? 'translate-x-6' : 'translate-x-1'
                }`}
              />
            </button>
          </SettingField>

          <SettingField
            label="720p Minimum"
            description="Minimum MB per minute for 720p releases"
          >
            <div className="flex items-center gap-2">
              <input
                type="number"
                step="0.5"
                min="0"
                className="w-20 rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm"
                value={settings.sizeFilter?.minMbPerMin720p ?? 12.5}
                disabled={!settings.sizeFilter?.enabled}
                onChange={(e) =>
                  updateMutation.mutate({
                    sizeFilter: { minMbPerMin720p: Number.parseFloat(e.target.value) },
                  })
                }
              />
              <span className="text-sm text-muted">MB/min</span>
            </div>
          </SettingField>

          <SettingField
            label="1080p Minimum"
            description="Minimum MB per minute for 1080p releases"
          >
            <div className="flex items-center gap-2">
              <input
                type="number"
                step="0.5"
                min="0"
                className="w-20 rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm"
                value={settings.sizeFilter?.minMbPerMin1080p ?? 12.5}
                disabled={!settings.sizeFilter?.enabled}
                onChange={(e) =>
                  updateMutation.mutate({
                    sizeFilter: { minMbPerMin1080p: Number.parseFloat(e.target.value) },
                  })
                }
              />
              <span className="text-sm text-muted">MB/min</span>
            </div>
          </SettingField>

          <SettingField
            label="2160p (4K) Minimum"
            description="Minimum MB per minute for 4K releases"
          >
            <div className="flex items-center gap-2">
              <input
                type="number"
                step="0.5"
                min="0"
                className="w-20 rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm"
                value={settings.sizeFilter?.minMbPerMin2160p ?? 34.5}
                disabled={!settings.sizeFilter?.enabled}
                onChange={(e) =>
                  updateMutation.mutate({
                    sizeFilter: { minMbPerMin2160p: Number.parseFloat(e.target.value) },
                  })
                }
              />
              <span className="text-sm text-muted">MB/min</span>
            </div>
          </SettingField>
        </SettingsSection>

        {/* Metadata */}
        <SettingsSection title="Metadata" description="Configure metadata providers">
          <SettingField
            label="Provider Priority"
            description="Order in which metadata providers are queried"
            locked={isLocked('metadata.providers')}
          >
            <div className="text-sm text-muted font-mono">
              {settings.metadata.providers.join(' → ')}
            </div>
          </SettingField>

          <SettingField label="TMDB API Key" locked={isLocked('metadata.tmdb.apiKey')}>
            <input
              type="password"
              className="w-64 rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm"
              value={settings.metadata.tmdb.apiKey || ''}
              disabled={isLocked('metadata.tmdb.apiKey')}
              placeholder="Enter TMDB API key"
              onChange={(e) =>
                updateMutation.mutate({ metadata: { tmdb: { apiKey: e.target.value } } })
              }
            />
          </SettingField>

          <SettingField label="TVDB API Key" locked={isLocked('metadata.tvdb.apiKey')}>
            <input
              type="password"
              className="w-64 rounded-md bg-background border border-border px-3 py-1.5 text-foreground text-sm"
              value={settings.metadata.tvdb.apiKey || ''}
              disabled={isLocked('metadata.tvdb.apiKey')}
              placeholder="Enter TVDB API key"
              onChange={(e) =>
                updateMutation.mutate({ metadata: { tvdb: { apiKey: e.target.value } } })
              }
            />
          </SettingField>
        </SettingsSection>
      </div>
    </div>
  )
}

import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import type { Indexer, CreateIndexer } from '@jaeger/shared/types/indexer'

const API_BASE_URL = 'http://localhost:5001'

type IndexersResponse = {
  indexers: Indexer[]
}

type RegistryResponse = {
  indexers: Array<{ domain: string; name: string; protocol: string }>
}

type TestResult = {
  success: boolean
  version?: string
  error?: string
}

async function fetchIndexers(): Promise<IndexersResponse> {
  const response = await fetch(`${API_BASE_URL}/api/indexers`, {
    credentials: 'include',
  })
  if (!response.ok) throw new Error('Failed to fetch indexers')
  return response.json()
}

async function fetchRegistry(): Promise<RegistryResponse> {
  const response = await fetch(`${API_BASE_URL}/api/indexers/registry`, {
    credentials: 'include',
  })
  if (!response.ok) throw new Error('Failed to fetch registry')
  return response.json()
}

async function addIndexer(data: CreateIndexer): Promise<{ indexer: Indexer }> {
  const response = await fetch(`${API_BASE_URL}/api/indexers`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'include',
    body: JSON.stringify(data),
  })
  if (!response.ok) throw new Error('Failed to add indexer')
  return response.json()
}

async function updateIndexer(
  id: string,
  updates: Partial<Indexer>
): Promise<{ indexer: Indexer }> {
  const response = await fetch(`${API_BASE_URL}/api/indexers/${id}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'include',
    body: JSON.stringify(updates),
  })
  if (!response.ok) throw new Error('Failed to update indexer')
  return response.json()
}

async function deleteIndexer(id: string): Promise<void> {
  const response = await fetch(`${API_BASE_URL}/api/indexers/${id}`, {
    method: 'DELETE',
    credentials: 'include',
  })
  if (!response.ok) throw new Error('Failed to delete indexer')
}

async function testConnection(data: {
  protocol: string
  url: string
  apiKey: string
}): Promise<TestResult> {
  const response = await fetch(`${API_BASE_URL}/api/indexers/test`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    credentials: 'include',
    body: JSON.stringify(data),
  })
  if (!response.ok) throw new Error('Failed to test connection')
  return response.json()
}

type IndexerFormData = {
  name: string
  protocol: 'newznab'
  url: string
  apiKey: string
  enabled: boolean
  categories?: {
    movies?: number[]
    tv?: number[]
  }
}

const defaultFormData: IndexerFormData = {
  name: '',
  protocol: 'newznab',
  url: '',
  apiKey: '',
  enabled: true,
}

type IndexerModalProps = {
  isOpen: boolean
  onClose: () => void
  onSubmit: (data: CreateIndexer) => void
  initialData?: Indexer
  isLoading: boolean
  registry: RegistryResponse['indexers']
}

function IndexerModal({
  isOpen,
  onClose,
  onSubmit,
  initialData,
  isLoading,
  registry,
}: IndexerModalProps) {
  const [formData, setFormData] = useState<IndexerFormData>(
    initialData
      ? {
          name: initialData.name,
          protocol: initialData.protocol,
          url: initialData.url,
          apiKey: '',
          enabled: initialData.enabled,
          categories: initialData.categories,
        }
      : defaultFormData
  )
  const [showApiKey, setShowApiKey] = useState(false)
  const [testResult, setTestResult] = useState<TestResult | null>(null)
  const [isTesting, setIsTesting] = useState(false)
  const [selectedRegistry, setSelectedRegistry] = useState<string>('')
  const [searchQuery, setSearchQuery] = useState('')

  const filteredRegistry = registry.filter(
    (r) =>
      r.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
      r.domain.toLowerCase().includes(searchQuery.toLowerCase())
  )

  const handleRegistrySelect = (domain: string) => {
    const entry = registry.find((r) => r.domain === domain)
    if (entry) {
      setSelectedRegistry(domain)
      setFormData({
        ...formData,
        name: entry.name,
        protocol: entry.protocol as 'newznab',
        url: `https://${domain}`,
      })
    }
  }

  const handleTest = async () => {
    if (!formData.url || !formData.apiKey) return
    setIsTesting(true)
    setTestResult(null)
    try {
      const result = await testConnection({
        protocol: formData.protocol,
        url: formData.url,
        apiKey: formData.apiKey,
      })
      setTestResult(result)
    } catch (error) {
      setTestResult({
        success: false,
        error: error instanceof Error ? error.message : 'Test failed',
      })
    } finally {
      setIsTesting(false)
    }
  }

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    const data: CreateIndexer = {
      name: formData.name,
      protocol: formData.protocol,
      url: formData.url,
      apiKey: formData.apiKey,
      enabled: formData.enabled,
    }
    if (formData.categories) {
      data.categories = formData.categories
    }
    onSubmit(data)
  }

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={onClose} />
      <div className="relative bg-card border border-border rounded-lg p-6 w-full max-w-md mx-4 shadow-xl">
        <h2 className="text-lg font-semibold text-foreground mb-4">
          {initialData ? 'Edit Indexer' : 'Add Indexer'}
        </h2>

        <form onSubmit={handleSubmit} className="space-y-4">
          {!initialData && (
            <div>
              <label className="block text-sm font-medium text-foreground mb-1">
                Select from Known Indexers
              </label>
              <input
                type="text"
                className="w-full rounded-md bg-background border border-border px-3 py-2 text-foreground text-sm mb-2"
                placeholder="Search indexers..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
              <div className="max-h-32 overflow-y-auto border border-border rounded-md bg-background">
                {filteredRegistry.map((entry) => (
                  <button
                    key={entry.domain}
                    type="button"
                    onClick={() => handleRegistrySelect(entry.domain)}
                    className={`w-full text-left px-3 py-2 text-sm hover:bg-accent/10 transition-colors truncate ${
                      selectedRegistry === entry.domain ? 'bg-accent/20 text-accent' : 'text-foreground'
                    }`}
                  >
                    <span className="font-medium">{entry.name}</span>
                    <span className="text-muted ml-2 text-xs">{entry.domain}</span>
                  </button>
                ))}
                {filteredRegistry.length === 0 && (
                  <div className="px-3 py-2 text-sm text-muted">No matching indexers</div>
                )}
              </div>
              <p className="text-xs text-muted mt-1">Or enter custom details below</p>
            </div>
          )}

          <div>
            <label className="block text-sm font-medium text-foreground mb-1">Name</label>
            <input
              type="text"
              required
              className="w-full rounded-md bg-background border border-border px-3 py-2 text-foreground text-sm"
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              placeholder="NZBgeek"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-foreground mb-1">Protocol</label>
            <select
              className="w-full rounded-md bg-background border border-border px-3 py-2 text-foreground text-sm"
              value={formData.protocol}
              onChange={(e) => setFormData({ ...formData, protocol: e.target.value as 'newznab' })}
            >
              <option value="newznab">Newznab</option>
            </select>
          </div>

          <div>
            <label className="block text-sm font-medium text-foreground mb-1">URL</label>
            <input
              type="url"
              required
              className="w-full rounded-md bg-background border border-border px-3 py-2 text-foreground text-sm"
              value={formData.url}
              onChange={(e) => setFormData({ ...formData, url: e.target.value })}
              placeholder="https://api.nzbgeek.info"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-foreground mb-1">API Key</label>
            <div className="flex gap-2">
              <input
                type={showApiKey ? 'text' : 'password'}
                required={!initialData}
                className="flex-1 rounded-md bg-background border border-border px-3 py-2 text-foreground text-sm"
                value={formData.apiKey}
                onChange={(e) => setFormData({ ...formData, apiKey: e.target.value })}
                placeholder={initialData ? 'Leave empty to keep existing' : 'Enter API key'}
              />
              <button
                type="button"
                onClick={() => setShowApiKey(!showApiKey)}
                className="px-3 py-2 text-sm rounded-md border border-border text-muted hover:text-foreground transition-colors"
              >
                {showApiKey ? 'Hide' : 'Show'}
              </button>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <input
              type="checkbox"
              id="enabled"
              checked={formData.enabled}
              onChange={(e) => setFormData({ ...formData, enabled: e.target.checked })}
              className="rounded border-border"
            />
            <label htmlFor="enabled" className="text-sm text-foreground">
              Enabled
            </label>
          </div>

          {testResult && (
            <div
              className={`p-3 rounded-md text-sm ${
                testResult.success
                  ? 'bg-green-500/10 text-green-400 border border-green-500/30'
                  : 'bg-red-500/10 text-red-400 border border-red-500/30'
              }`}
            >
              {testResult.success
                ? `Connected! Newznab version ${testResult.version || 'unknown'}`
                : `Connection failed: ${testResult.error}`}
            </div>
          )}

          <div className="flex gap-2 pt-2">
            <button
              type="button"
              onClick={handleTest}
              disabled={isTesting || !formData.url || !formData.apiKey}
              className="px-4 py-2 text-sm rounded-md border border-border text-muted hover:text-foreground transition-colors disabled:opacity-50"
            >
              {isTesting ? 'Testing...' : 'Test Connection'}
            </button>
            <div className="flex-1" />
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-sm rounded-md border border-border text-muted hover:text-foreground transition-colors"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={isLoading}
              className="px-4 py-2 text-sm rounded-md bg-accent text-white hover:bg-accent/90 transition-colors disabled:opacity-50"
            >
              {isLoading ? 'Saving...' : initialData ? 'Save' : 'Add'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

type IndexerCardProps = {
  indexer: Indexer
  onEdit: () => void
  onDelete: () => void
  onToggle: (enabled: boolean) => void
}

function IndexerCard({ indexer, onEdit, onDelete, onToggle }: IndexerCardProps) {
  return (
    <div className="bg-card border border-border rounded-lg p-4">
      <div className="flex items-start justify-between">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h3 className="font-medium text-foreground">{indexer.name}</h3>
            <span
              className={`px-2 py-0.5 text-xs rounded-full ${
                indexer.enabled
                  ? 'bg-green-500/20 text-green-400'
                  : 'bg-gray-500/20 text-gray-400'
              }`}
            >
              {indexer.enabled ? 'Enabled' : 'Disabled'}
            </span>
          </div>
          <p className="text-sm text-muted mt-1">{indexer.protocol.toUpperCase()}</p>
          <p className="text-xs text-muted font-mono mt-1 truncate">{indexer.url}</p>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => onToggle(!indexer.enabled)}
            className="p-2 text-muted hover:text-foreground transition-colors"
            title={indexer.enabled ? 'Disable' : 'Enable'}
          >
            {indexer.enabled ? (
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M18.364 18.364A9 9 0 005.636 5.636m12.728 12.728A9 9 0 015.636 5.636m12.728 12.728L5.636 5.636"
                />
              </svg>
            ) : (
              <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M5 13l4 4L19 7"
                />
              </svg>
            )}
          </button>
          <button
            type="button"
            onClick={onEdit}
            className="p-2 text-muted hover:text-foreground transition-colors"
            title="Edit"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
              />
            </svg>
          </button>
          <button
            type="button"
            onClick={onDelete}
            className="p-2 text-muted hover:text-red-400 transition-colors"
            title="Delete"
          >
            <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
              />
            </svg>
          </button>
        </div>
      </div>
    </div>
  )
}

export default function Indexers() {
  const queryClient = useQueryClient()
  const [isModalOpen, setIsModalOpen] = useState(false)
  const [editingIndexer, setEditingIndexer] = useState<Indexer | undefined>()

  const { data, isLoading, error } = useQuery({
    queryKey: ['indexers'],
    queryFn: fetchIndexers,
  })

  const { data: registryData } = useQuery({
    queryKey: ['indexers-registry'],
    queryFn: fetchRegistry,
  })

  const addMutation = useMutation({
    mutationFn: addIndexer,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['indexers'] })
      setIsModalOpen(false)
    },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, updates }: { id: string; updates: Partial<Indexer> }) =>
      updateIndexer(id, updates),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['indexers'] })
      setIsModalOpen(false)
      setEditingIndexer(undefined)
    },
  })

  const deleteMutation = useMutation({
    mutationFn: deleteIndexer,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['indexers'] })
    },
  })

  const handleAdd = () => {
    setEditingIndexer(undefined)
    setIsModalOpen(true)
  }

  const handleEdit = (indexer: Indexer) => {
    setEditingIndexer(indexer)
    setIsModalOpen(true)
  }

  const handleSubmit = (formData: CreateIndexer) => {
    if (editingIndexer) {
      const updates: Partial<Indexer> = { ...formData }
      if (!formData.apiKey) {
        delete updates.apiKey
      }
      updateMutation.mutate({ id: editingIndexer.id, updates })
    } else {
      addMutation.mutate(formData)
    }
  }

  const handleDelete = (indexer: Indexer) => {
    if (confirm(`Delete "${indexer.name}"?`)) {
      deleteMutation.mutate(indexer.id)
    }
  }

  const handleToggle = (indexer: Indexer, enabled: boolean) => {
    updateMutation.mutate({ id: indexer.id, updates: { enabled } })
  }

  if (isLoading) {
    return (
      <div className="min-h-screen p-6 flex items-center justify-center">
        <div className="h-8 w-8 animate-spin rounded-full border-2 border-accent border-t-transparent" />
      </div>
    )
  }

  if (error) {
    return (
      <div className="min-h-screen p-6">
        <div className="rounded-lg bg-card border border-red-500/50 p-6 text-red-400">
          Failed to load indexers: {error instanceof Error ? error.message : 'Unknown error'}
        </div>
      </div>
    )
  }

  const indexers = data?.indexers || []
  const registry = registryData?.indexers || []

  return (
    <div className="min-h-screen p-6">
      <div className="mb-6 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-foreground">Indexers</h1>
          <p className="text-sm text-muted">Configure Usenet indexers for searching releases</p>
        </div>
        <button
          type="button"
          onClick={handleAdd}
          className="px-4 py-2 text-sm rounded-md bg-accent text-white hover:bg-accent/90 transition-colors flex items-center gap-2"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
          </svg>
          Add Indexer
        </button>
      </div>

      <div className="max-w-3xl">
        {indexers.length === 0 ? (
          <div className="bg-card border border-border rounded-lg p-8 text-center">
            <div className="text-muted mb-2">No indexers configured</div>
            <p className="text-sm text-muted">
              Add an indexer to start searching for releases
            </p>
          </div>
        ) : (
          <div className="space-y-4">
            {indexers.map((indexer) => (
              <IndexerCard
                key={indexer.id}
                indexer={indexer}
                onEdit={() => handleEdit(indexer)}
                onDelete={() => handleDelete(indexer)}
                onToggle={(enabled) => handleToggle(indexer, enabled)}
              />
            ))}
          </div>
        )}
      </div>

      <IndexerModal
        isOpen={isModalOpen}
        onClose={() => {
          setIsModalOpen(false)
          setEditingIndexer(undefined)
        }}
        onSubmit={handleSubmit}
        initialData={editingIndexer}
        isLoading={addMutation.isPending || updateMutation.isPending}
        registry={registry}
      />
    </div>
  )
}

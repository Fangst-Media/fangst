import { useState } from 'react'
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import type { Downloader, CreateDownloader } from '@jaeger/shared/types/settings'
import { apiFetch, apiPost } from '../lib/api-client'

type DownloadersResponse = {
  downloaders: Downloader[]
}

type TestResult = {
  success: boolean
  version?: string
  error?: string
}

async function fetchDownloaders(): Promise<DownloadersResponse> {
  return apiFetch('/api/downloaders')
}

async function addDownloader(data: CreateDownloader): Promise<{ downloader: Downloader }> {
  return apiPost('/api/downloaders', data)
}

async function updateDownloader(
  id: string,
  updates: Partial<Downloader>
): Promise<{ downloader: Downloader }> {
  return apiFetch(`/api/downloaders/${id}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(updates),
  })
}

async function deleteDownloader(id: string): Promise<void> {
  return apiFetch(`/api/downloaders/${id}`, { method: 'DELETE' })
}

async function testConnection(data: {
  type: string
  url: string
  apiKey: string
}): Promise<TestResult> {
  return apiPost('/api/downloaders/test', data)
}

type DownloaderFormData = {
  name: string
  type: 'sabnzbd'
  url: string
  apiKey: string
  enabled: boolean
  categories?: {
    movies?: string
    tv?: string
  }
}

const defaultFormData: DownloaderFormData = {
  name: '',
  type: 'sabnzbd',
  url: '',
  apiKey: '',
  enabled: true,
  categories: {
    movies: '',
    tv: '',
  },
}

type DownloaderModalProps = {
  isOpen: boolean
  onClose: () => void
  onSubmit: (data: CreateDownloader) => void
  initialData?: Downloader
  isLoading: boolean
}

function DownloaderModal({
  isOpen,
  onClose,
  onSubmit,
  initialData,
  isLoading,
}: DownloaderModalProps) {
  const [formData, setFormData] = useState<DownloaderFormData>(
    initialData
      ? {
          name: initialData.name,
          type: initialData.type,
          url: initialData.url,
          apiKey: '',
          enabled: initialData.enabled,
          categories: initialData.categories || { movies: '', tv: '' },
        }
      : defaultFormData
  )
  const [showApiKey, setShowApiKey] = useState(false)
  const [testResult, setTestResult] = useState<TestResult | null>(null)
  const [isTesting, setIsTesting] = useState(false)

  const handleTest = async () => {
    if (!formData.url || !formData.apiKey) return
    setIsTesting(true)
    setTestResult(null)
    try {
      const result = await testConnection({
        type: formData.type,
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
    const data: CreateDownloader = {
      name: formData.name,
      type: formData.type,
      url: formData.url,
      apiKey: formData.apiKey,
      enabled: formData.enabled,
    }
    if (formData.categories?.movies || formData.categories?.tv) {
      data.categories = {}
      if (formData.categories.movies) data.categories.movies = formData.categories.movies
      if (formData.categories.tv) data.categories.tv = formData.categories.tv
    }
    onSubmit(data)
  }

  if (!isOpen) return null

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={onClose} />
      <div className="relative bg-card border border-border rounded-lg p-6 w-full max-w-md mx-4 shadow-xl">
        <h2 className="text-lg font-semibold text-foreground mb-4">
          {initialData ? 'Edit Downloader' : 'Add Downloader'}
        </h2>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-sm font-medium text-foreground mb-1">Name</label>
            <input
              type="text"
              required
              className="w-full rounded-md bg-background border border-border px-3 py-2 text-foreground text-sm"
              value={formData.name}
              onChange={(e) => setFormData({ ...formData, name: e.target.value })}
              placeholder="Main SABnzbd"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-foreground mb-1">Type</label>
            <select
              className="w-full rounded-md bg-background border border-border px-3 py-2 text-foreground text-sm"
              value={formData.type}
              onChange={(e) => setFormData({ ...formData, type: e.target.value as 'sabnzbd' })}
            >
              <option value="sabnzbd">SABnzbd</option>
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
              placeholder="http://localhost:8080"
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

          <div className="border-t border-border pt-4">
            <label className="block text-sm font-medium text-foreground mb-2">
              Categories (optional)
            </label>
            <div className="grid grid-cols-2 gap-3">
              <div>
                <label className="block text-xs text-muted mb-1">Movies</label>
                <input
                  type="text"
                  className="w-full rounded-md bg-background border border-border px-3 py-2 text-foreground text-sm"
                  value={formData.categories?.movies || ''}
                  onChange={(e) =>
                    setFormData({
                      ...formData,
                      categories: { ...formData.categories, movies: e.target.value },
                    })
                  }
                  placeholder="movies"
                />
              </div>
              <div>
                <label className="block text-xs text-muted mb-1">TV Shows</label>
                <input
                  type="text"
                  className="w-full rounded-md bg-background border border-border px-3 py-2 text-foreground text-sm"
                  value={formData.categories?.tv || ''}
                  onChange={(e) =>
                    setFormData({
                      ...formData,
                      categories: { ...formData.categories, tv: e.target.value },
                    })
                  }
                  placeholder="tv"
                />
              </div>
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
                ? `Connected! SABnzbd version ${testResult.version}`
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

type DownloaderCardProps = {
  downloader: Downloader
  onEdit: () => void
  onDelete: () => void
  onToggle: (enabled: boolean) => void
}

function DownloaderCard({ downloader, onEdit, onDelete, onToggle }: DownloaderCardProps) {
  return (
    <div className="bg-card border border-border rounded-lg p-4">
      <div className="flex items-start justify-between">
        <div className="flex-1">
          <div className="flex items-center gap-2">
            <h3 className="font-medium text-foreground">{downloader.name}</h3>
            <span
              className={`px-2 py-0.5 text-xs rounded-full ${
                downloader.enabled
                  ? 'bg-green-500/20 text-green-400'
                  : 'bg-gray-500/20 text-gray-400'
              }`}
            >
              {downloader.enabled ? 'Enabled' : 'Disabled'}
            </span>
          </div>
          <p className="text-sm text-muted mt-1">{downloader.type.toUpperCase()}</p>
          <p className="text-xs text-muted font-mono mt-1">{downloader.url}</p>
          {downloader.categories && (
            <div className="flex gap-2 mt-2">
              {downloader.categories.movies && (
                <span className="text-xs px-2 py-0.5 bg-background rounded border border-border text-muted">
                  Movies: {downloader.categories.movies}
                </span>
              )}
              {downloader.categories.tv && (
                <span className="text-xs px-2 py-0.5 bg-background rounded border border-border text-muted">
                  TV: {downloader.categories.tv}
                </span>
              )}
            </div>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => onToggle(!downloader.enabled)}
            className="p-2 text-muted hover:text-foreground transition-colors"
            title={downloader.enabled ? 'Disable' : 'Enable'}
          >
            {downloader.enabled ? (
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

export default function Downloaders() {
  const queryClient = useQueryClient()
  const [isModalOpen, setIsModalOpen] = useState(false)
  const [editingDownloader, setEditingDownloader] = useState<Downloader | undefined>()

  const { data, isLoading, error } = useQuery({
    queryKey: ['downloaders'],
    queryFn: fetchDownloaders,
  })

  const addMutation = useMutation({
    mutationFn: addDownloader,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['downloaders'] })
      setIsModalOpen(false)
    },
  })

  const updateMutation = useMutation({
    mutationFn: ({ id, updates }: { id: string; updates: Partial<Downloader> }) =>
      updateDownloader(id, updates),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['downloaders'] })
      setIsModalOpen(false)
      setEditingDownloader(undefined)
    },
  })

  const deleteMutation = useMutation({
    mutationFn: deleteDownloader,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['downloaders'] })
    },
  })

  const handleAdd = () => {
    setEditingDownloader(undefined)
    setIsModalOpen(true)
  }

  const handleEdit = (downloader: Downloader) => {
    setEditingDownloader(downloader)
    setIsModalOpen(true)
  }

  const handleSubmit = (formData: CreateDownloader) => {
    if (editingDownloader) {
      const updates: Partial<Downloader> = { ...formData }
      if (!formData.apiKey) {
        delete updates.apiKey
      }
      updateMutation.mutate({ id: editingDownloader.id, updates })
    } else {
      addMutation.mutate(formData)
    }
  }

  const handleDelete = (downloader: Downloader) => {
    if (confirm(`Delete "${downloader.name}"?`)) {
      deleteMutation.mutate(downloader.id)
    }
  }

  const handleToggle = (downloader: Downloader, enabled: boolean) => {
    updateMutation.mutate({ id: downloader.id, updates: { enabled } })
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
          Failed to load downloaders: {error instanceof Error ? error.message : 'Unknown error'}
        </div>
      </div>
    )
  }

  const downloaders = data?.downloaders || []

  return (
    <div className="min-h-screen p-6">
      <div className="mb-6 flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold text-foreground">Downloaders</h1>
          <p className="text-sm text-muted">Configure download clients for media acquisition</p>
        </div>
        <button
          type="button"
          onClick={handleAdd}
          className="px-4 py-2 text-sm rounded-md bg-accent text-white hover:bg-accent/90 transition-colors flex items-center gap-2"
        >
          <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
          </svg>
          Add Downloader
        </button>
      </div>

      <div className="max-w-3xl">
        {downloaders.length === 0 ? (
          <div className="bg-card border border-border rounded-lg p-8 text-center">
            <div className="text-muted mb-2">No downloaders configured</div>
            <p className="text-sm text-muted">
              Add a downloader to start managing your media downloads
            </p>
          </div>
        ) : (
          <div className="space-y-4">
            {downloaders.map((downloader) => (
              <DownloaderCard
                key={downloader.id}
                downloader={downloader}
                onEdit={() => handleEdit(downloader)}
                onDelete={() => handleDelete(downloader)}
                onToggle={(enabled) => handleToggle(downloader, enabled)}
              />
            ))}
          </div>
        )}
      </div>

      <DownloaderModal
        isOpen={isModalOpen}
        onClose={() => {
          setIsModalOpen(false)
          setEditingDownloader(undefined)
        }}
        onSubmit={handleSubmit}
        initialData={editingDownloader}
        isLoading={addMutation.isPending || updateMutation.isPending}
      />
    </div>
  )
}

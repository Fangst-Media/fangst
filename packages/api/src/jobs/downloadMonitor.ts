import { requestService } from '../services/requests'
import { configService } from '../services/config'
import { SABnzbdClient, type SABnzbdHistorySlot } from '@jaeger/shared/clients/sabnzbd'
import { runLibraryScan } from '../routes/media'

export async function runDownloadMonitor(): Promise<{
  checked: number
  completed: number
  failed: number
  librarySync?: { added: number; updated: number; removed: number }
}> {
  console.log('[DownloadMonitor] Starting download monitor job...')

  const downloadingRequests = await requestService.getDownloading()
  console.log(`[DownloadMonitor] Found ${downloadingRequests.length} downloading requests`)

  if (downloadingRequests.length === 0) {
    return { checked: 0, completed: 0, failed: 0 }
  }

  const downloaders = configService.getEnabledDownloaders()
  if (downloaders.length === 0) {
    console.log('[DownloadMonitor] No downloaders configured')
    return { checked: 0, completed: 0, failed: 0 }
  }

  let completed = 0
  let failed = 0

  // Collect history from all SABnzbd downloaders
  const allHistory: SABnzbdHistorySlot[] = []

  for (const downloader of downloaders) {
    if (downloader.type === 'sabnzbd') {
      try {
        const client = new SABnzbdClient(downloader.url, downloader.apiKey)
        const history = await client.getHistory(50)
        allHistory.push(...history.slots)
      } catch (error) {
        console.error(`[DownloadMonitor] Failed to get history from ${downloader.name}:`, error)
      }
    }
  }

  console.log(`[DownloadMonitor] Got ${allHistory.length} history items from SABnzbd`)

  // Check each downloading request against history
  for (const request of downloadingRequests) {
    const match = findMatchingHistorySlot(request.nzoId, request.releaseName, request.title, allHistory)

    if (match) {
      if (match.status === 'Completed') {
        await requestService.updateStatus(request.id, 'completed')
        console.log(`[DownloadMonitor] Completed: ${request.title}`)
        completed++
      } else if (match.status === 'Failed') {
        await requestService.updateStatus(request.id, 'failed')
        console.log(`[DownloadMonitor] Failed: ${request.title} - ${match.failMessage ?? 'Unknown error'}`)
        failed++
      }
    }
  }

  console.log(`[DownloadMonitor] Complete: checked=${downloadingRequests.length}, completed=${completed}, failed=${failed}`)

  // Trigger library scan if any downloads completed
  let librarySync: { added: number; updated: number; removed: number } | undefined
  if (completed > 0) {
    console.log('[DownloadMonitor] Triggering library scan...')
    try {
      librarySync = await runLibraryScan()
      console.log(`[DownloadMonitor] Library scan: added=${librarySync.added}, updated=${librarySync.updated}, removed=${librarySync.removed}`)
    } catch (error) {
      console.error('[DownloadMonitor] Library scan failed:', error)
    }
  }

  return { checked: downloadingRequests.length, completed, failed, librarySync }
}

function normalizeForMatching(name: string): string {
  return name
    .toLowerCase()
    .replace(/['"!?]/g, '')
    .replace(/[:\s]+/g, '.')
    .replace(/\.+/g, '.')
}

function findMatchingHistorySlot(
  nzoId: string | null,
  releaseName: string | null,
  requestTitle: string,
  history: SABnzbdHistorySlot[]
): SABnzbdHistorySlot | undefined {
  // Primary match: by nzoId (exact)
  if (nzoId) {
    const match = history.find((slot) => slot.id === nzoId)
    if (match) return match
  }

  // Secondary: by release name (fuzzy)
  if (releaseName) {
    const normalizedRelease = normalizeForMatching(releaseName)
    const match = history.find((slot) => {
      const slotName = normalizeForMatching(slot.name)
      return slotName.includes(normalizedRelease) || normalizedRelease.includes(slotName)
    })
    if (match) return match
  }

  // Tertiary: by request title (for requests without release name)
  const normalizedTitle = normalizeForMatching(requestTitle)
  const match = history.find((slot) => {
    const slotName = normalizeForMatching(slot.name)
    return slotName.includes(normalizedTitle) || normalizedTitle.includes(slotName.split('.').slice(0, 3).join('.'))
  })

  return match
}

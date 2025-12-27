import { runSearchJob } from '../jobs/searchRequests'
import { runDownloadMonitor } from '../jobs/downloadMonitor'
import { logger } from './logger'

const SEARCH_JOB_INTERVAL = 15 * 60 * 1000 // 15 minutes
const DOWNLOAD_MONITOR_INTERVAL = 2 * 60 * 1000 // 2 minutes

let searchJobInterval: ReturnType<typeof setInterval> | null = null
let downloadMonitorInterval: ReturnType<typeof setInterval> | null = null

let isSearchJobRunning = false
let isDownloadMonitorRunning = false

async function runSearchJobWithGuard() {
  if (isSearchJobRunning) {
    logger.debug('Search job already running, skipping')
    return
  }

  isSearchJobRunning = true
  try {
    const result = await runSearchJob()
    logger.info(result, 'Scheduled search job completed')
  } catch (error) {
    logger.error({ error: error instanceof Error ? error.message : error }, 'Scheduled search job failed')
  } finally {
    isSearchJobRunning = false
  }
}

async function runDownloadMonitorWithGuard() {
  if (isDownloadMonitorRunning) {
    logger.debug('Download monitor already running, skipping')
    return
  }

  isDownloadMonitorRunning = true
  try {
    const result = await runDownloadMonitor()
    if (result.checked > 0 || result.completed > 0 || result.failed > 0) {
      logger.info(result, 'Scheduled download monitor completed')
    }
  } catch (error) {
    logger.error({ error: error instanceof Error ? error.message : error }, 'Scheduled download monitor failed')
  } finally {
    isDownloadMonitorRunning = false
  }
}

export function startScheduler() {
  logger.info(
    {
      searchJobInterval: `${SEARCH_JOB_INTERVAL / 1000 / 60} minutes`,
      downloadMonitorInterval: `${DOWNLOAD_MONITOR_INTERVAL / 1000 / 60} minutes`,
    },
    'Starting job scheduler'
  )

  // Run download monitor immediately on startup, then on interval
  runDownloadMonitorWithGuard()
  downloadMonitorInterval = setInterval(runDownloadMonitorWithGuard, DOWNLOAD_MONITOR_INTERVAL)

  // Run search job on interval (not immediately - let download monitor settle first)
  searchJobInterval = setInterval(runSearchJobWithGuard, SEARCH_JOB_INTERVAL)
}

export function stopScheduler() {
  logger.info('Stopping job scheduler')

  if (searchJobInterval) {
    clearInterval(searchJobInterval)
    searchJobInterval = null
  }

  if (downloadMonitorInterval) {
    clearInterval(downloadMonitorInterval)
    downloadMonitorInterval = null
  }
}

// For triggering search immediately (e.g., after approval)
export async function triggerSearchJob() {
  return runSearchJobWithGuard()
}

export function getSchedulerStatus() {
  return {
    searchJob: {
      intervalMs: SEARCH_JOB_INTERVAL,
      isRunning: isSearchJobRunning,
    },
    downloadMonitor: {
      intervalMs: DOWNLOAD_MONITOR_INTERVAL,
      isRunning: isDownloadMonitorRunning,
    },
  }
}

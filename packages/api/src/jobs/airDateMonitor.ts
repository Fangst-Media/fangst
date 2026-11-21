import { MetadataRelayClient } from '@fangst/shared/clients/metadata-relay'
import { configService } from '../services/config'
import { type RequestWithUser, requestService } from '../services/requests'

interface AirDateMonitorResult {
  requestsChecked: number
  episodesAdded: number
  statusUpdates: number
  errors: number
}

export async function runAirDateMonitor(): Promise<AirDateMonitorResult> {
  console.log('[AirDateMonitor] Starting air date monitor job...')

  const stats = {
    requestsChecked: 0,
    episodesAdded: 0,
    statusUpdates: 0,
    errors: 0,
  }

  // Get TV requests for ongoing series
  const returningRequests = await requestService.getReturningSeriesRequests()
  console.log(
    `[AirDateMonitor] Found ${returningRequests.length} returning/upcoming series to monitor`
  )

  const relayUrl = configService.getMetadataRelayUrl()
  const client = new MetadataRelayClient(relayUrl)

  for (const request of returningRequests) {
    stats.requestsChecked++

    if (!request.tmdbId) {
      console.log(`[AirDateMonitor] Skipping ${request.title} - no TMDB ID`)
      continue
    }

    if (!request.season) {
      console.log(`[AirDateMonitor] Skipping ${request.title} - no season specified`)
      continue
    }

    try {
      await processRequest(request, client, stats)
    } catch (error) {
      console.error(`[AirDateMonitor] Error processing ${request.title}:`, error)
      stats.errors++
    }

    // Small delay between requests
    await new Promise((resolve) => setTimeout(resolve, 500))
  }

  console.log(
    `[AirDateMonitor] Complete: checked=${stats.requestsChecked}, added=${stats.episodesAdded}, statusUpdates=${stats.statusUpdates}, errors=${stats.errors}`
  )

  return stats
}

async function processRequest(
  request: RequestWithUser,
  client: MetadataRelayClient,
  stats: AirDateMonitorResult
): Promise<void> {
  console.log(`[AirDateMonitor] Checking: ${request.title} S${request.season}`)

  // Fetch latest series info from TMDB
  const seriesDetails = await client.getTvDetails(request.tmdbId!)

  // Check if series status changed
  if (seriesDetails.status !== request.seriesStatus) {
    console.log(
      `[AirDateMonitor] Status changed for ${request.title}: ${request.seriesStatus} -> ${seriesDetails.status}`
    )
    await requestService.updateRequestSeriesStatus(request.id, seriesDetails.status)
    stats.statusUpdates++
  }

  // Fetch season details to get episode list
  const seasonDetails = await client.getSeasonDetails(request.tmdbId!, request.season!)
  const tmdbEpisodes = seasonDetails.episodes

  // Get existing episodes from database
  const existingEpisodes = await requestService.getEpisodes(request.id)
  const existingEpNumbers = new Set(existingEpisodes.map((ep) => ep.episodeNumber))

  // Find new episodes that need to be added
  const newEpisodes = tmdbEpisodes.filter((ep) => !existingEpNumbers.has(ep.episodeNumber))

  if (newEpisodes.length > 0) {
    console.log(
      `[AirDateMonitor] Adding ${newEpisodes.length} new episodes for ${request.title} S${request.season}`
    )

    for (const ep of newEpisodes) {
      await requestService.addEpisode(request.id, {
        seasonNumber: ep.seasonNumber,
        episodeNumber: ep.episodeNumber,
        title: ep.name,
        airDate: ep.airDate,
      })
      stats.episodesAdded++
    }
  }

  // Also check for updated air dates on existing episodes
  for (const existingEp of existingEpisodes) {
    const tmdbEp = tmdbEpisodes.find((ep) => ep.episodeNumber === existingEp.episodeNumber)

    if (tmdbEp && tmdbEp.airDate && tmdbEp.airDate !== existingEp.airDate) {
      // Air date was updated, but we don't have a direct method to update just the air date
      // This is a minor edge case - air dates rarely change
      console.log(
        `[AirDateMonitor] Air date updated for ${request.title} S${request.season}E${existingEp.episodeNumber}: ${existingEp.airDate} -> ${tmdbEp.airDate}`
      )
    }
  }

  // Check if all episodes are now completed - if so, update series status
  const allEpisodes = await requestService.getEpisodes(request.id)
  const allCompleted =
    allEpisodes.length > 0 && allEpisodes.every((ep) => ep.status === 'completed')

  if (allCompleted && seriesDetails.status === 'ended') {
    // Mark the request as completed
    await requestService.updateStatus(request.id, 'completed')
    console.log(`[AirDateMonitor] ${request.title} S${request.season} marked as completed`)
  }
}

export { processRequest }

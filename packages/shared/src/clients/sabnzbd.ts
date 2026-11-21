import { z } from 'zod'

const sabnzbdVersionSchema = z.object({
  version: z.string(),
})

const sabnzbdQueueSlotSchema = z.object({
  nzo_id: z.string(),
  filename: z.string(),
  status: z.string(),
  mb: z.string(),
  mbleft: z.string(),
  percentage: z.string(),
  timeleft: z.string(),
  cat: z.string(),
})

const sabnzbdQueueSchema = z.object({
  queue: z.object({
    status: z.string(),
    speedlimit: z.string(),
    speed: z.string(),
    sizeleft: z.string(),
    timeleft: z.string(),
    noofslots: z.number(),
    slots: z.array(sabnzbdQueueSlotSchema),
  }),
})

const sabnzbdCategoriesSchema = z.object({
  categories: z.array(z.string()),
})

const sabnzbdHistorySlotSchema = z.object({
  nzo_id: z.string(),
  name: z.string(),
  status: z.string(),
  bytes: z.number(),
  size: z.string(),
  category: z.string(),
  completed: z.number(),
  download_time: z.number(),
  fail_message: z.string().optional(),
  storage: z.string().optional(),
})

const sabnzbdHistorySchema = z.object({
  history: z.object({
    noofslots: z.number(),
    day_size: z.string(),
    week_size: z.string(),
    month_size: z.string(),
    total_size: z.string(),
    slots: z.array(sabnzbdHistorySlotSchema),
  }),
})

export type SABnzbdQueueSlot = {
  id: string
  filename: string
  status: string
  sizeMB: number
  remainingMB: number
  percentage: number
  timeLeft: string
  category: string
}

export type SABnzbdQueue = {
  status: string
  speedLimit: string
  speed: string
  sizeLeft: string
  timeLeft: string
  totalSlots: number
  slots: SABnzbdQueueSlot[]
}

export type SABnzbdHistorySlot = {
  id: string
  name: string
  status: string
  bytes: number
  size: string
  category: string
  completedAt: Date
  downloadTime: number
  failMessage?: string
  storage?: string
}

export type SABnzbdHistory = {
  totalSlots: number
  daySize: string
  weekSize: string
  monthSize: string
  totalSize: string
  slots: SABnzbdHistorySlot[]
}

export type SABnzbdTestResult = {
  success: boolean
  version?: string
  error?: string
}

export class SABnzbdClient {
  private baseUrl: string
  private apiKey: string

  constructor(baseUrl: string, apiKey: string) {
    this.baseUrl = baseUrl.replace(/\/$/, '')
    this.apiKey = apiKey
  }

  private async request<T>(mode: string, params: Record<string, string> = {}): Promise<T> {
    const url = new URL(`${this.baseUrl}/api`)
    url.searchParams.set('apikey', this.apiKey)
    url.searchParams.set('output', 'json')
    url.searchParams.set('mode', mode)

    for (const [key, value] of Object.entries(params)) {
      url.searchParams.set(key, value)
    }

    const response = await fetch(url.toString())

    if (!response.ok) {
      throw new Error(`SABnzbd request failed: ${response.statusText}`)
    }

    return response.json() as Promise<T>
  }

  async testConnection(): Promise<SABnzbdTestResult> {
    try {
      const data = await this.request<unknown>('version')
      const parsed = sabnzbdVersionSchema.parse(data)

      return {
        success: true,
        version: parsed.version,
      }
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error',
      }
    }
  }

  async getQueue(): Promise<SABnzbdQueue> {
    const data = await this.request<unknown>('queue')
    const parsed = sabnzbdQueueSchema.parse(data)

    return {
      status: parsed.queue.status,
      speedLimit: parsed.queue.speedlimit,
      speed: parsed.queue.speed,
      sizeLeft: parsed.queue.sizeleft,
      timeLeft: parsed.queue.timeleft,
      totalSlots: parsed.queue.noofslots,
      slots: parsed.queue.slots.map((slot) => ({
        id: slot.nzo_id,
        filename: slot.filename,
        status: slot.status,
        sizeMB: Number.parseFloat(slot.mb),
        remainingMB: Number.parseFloat(slot.mbleft),
        percentage: Number.parseFloat(slot.percentage),
        timeLeft: slot.timeleft,
        category: slot.cat,
      })),
    }
  }

  async getCategories(): Promise<string[]> {
    const data = await this.request<unknown>('get_cats')
    const parsed = sabnzbdCategoriesSchema.parse(data)
    return parsed.categories
  }

  async getHistory(limit = 20): Promise<SABnzbdHistory> {
    const data = await this.request<unknown>('history', { limit: limit.toString() })
    const parsed = sabnzbdHistorySchema.parse(data)

    return {
      totalSlots: parsed.history.noofslots,
      daySize: parsed.history.day_size,
      weekSize: parsed.history.week_size,
      monthSize: parsed.history.month_size,
      totalSize: parsed.history.total_size,
      slots: parsed.history.slots.map((slot) => ({
        id: slot.nzo_id,
        name: slot.name,
        status: slot.status,
        bytes: slot.bytes,
        size: slot.size,
        category: slot.category,
        completedAt: new Date(slot.completed * 1000),
        downloadTime: slot.download_time,
        failMessage: slot.fail_message,
        storage: slot.storage,
      })),
    }
  }

  async addNzbFromUrl(
    url: string,
    releaseName: string,
    category?: string
  ): Promise<{ nzoId: string; nzbContent: string }> {
    // Fetch the NZB file ourselves for better reliability
    const nzbResponse = await fetch(url)
    if (!nzbResponse.ok) {
      throw new Error(`Failed to fetch NZB: ${nzbResponse.status} ${nzbResponse.statusText}`)
    }

    const nzbContent = await nzbResponse.text()
    if (!nzbContent || nzbContent.length < 100 || !nzbContent.includes('<?xml')) {
      throw new Error('Invalid or empty NZB file received from indexer')
    }

    const result = await this.addNzbContent(nzbContent, releaseName, category)
    return { ...result, nzbContent }
  }

  async addNzbContent(
    nzbContent: string,
    releaseName: string,
    category?: string
  ): Promise<{ nzoId: string }> {
    // Upload NZB content directly to SABnzbd
    const formData = new FormData()
    formData.append('apikey', this.apiKey)
    formData.append('output', 'json')
    formData.append('mode', 'addfile')
    formData.append('nzbname', releaseName)
    if (category) {
      formData.append('cat', category)
    }
    const filename = `${releaseName}.nzb`
    formData.append('name', new Blob([nzbContent], { type: 'application/x-nzb' }), filename)

    const response = await fetch(`${this.baseUrl}/api`, {
      method: 'POST',
      body: formData,
    })

    if (!response.ok) {
      throw new Error(`SABnzbd request failed: ${response.statusText}`)
    }

    const data = (await response.json()) as { nzo_ids?: string[]; error?: string }
    if (data.error) {
      throw new Error(`SABnzbd error: ${data.error}`)
    }

    return { nzoId: data.nzo_ids?.[0] ?? '' }
  }

  async pause(): Promise<void> {
    await this.request('pause')
  }

  async resume(): Promise<void> {
    await this.request('resume')
  }

  async deleteItem(nzoId: string): Promise<void> {
    await this.request('queue', { name: 'delete', value: nzoId })
  }
}

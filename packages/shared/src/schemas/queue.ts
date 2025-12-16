import { z } from 'zod'

export const queueStatusSchema = z.enum([
  'downloading',
  'paused',
  'queued',
  'completed',
  'failed',
  'warning',
])

export const queueItemSchema = z.object({
  id: z.number(),
  title: z.string(),
  size: z.number(),
  sizeleft: z.number(),
  status: queueStatusSchema,
  trackedDownloadStatus: z.string().optional(),
  statusMessages: z.array(z.object({
    title: z.string(),
    messages: z.array(z.string()),
  })).optional(),
  downloadClient: z.string().optional(),
  indexer: z.string().optional(),
  protocol: z.enum(['torrent', 'usenet']).optional(),
  estimatedCompletionTime: z.string().optional(),
})

export type QueueStatus = z.infer<typeof queueStatusSchema>
export type QueueItem = z.infer<typeof queueItemSchema>

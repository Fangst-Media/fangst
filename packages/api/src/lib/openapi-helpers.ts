import { createRoute, z } from '@hono/zod-openapi'
import { errorResponseSchema, laneResponseSchema } from '@fangst/shared/schemas/metadata'

const errorContent = { 'application/json': { schema: errorResponseSchema } }

export function createLaneRoute(path: string, summary: string, tag: string) {
  return createRoute({
    method: 'get' as const,
    path,
    tags: [tag],
    summary,
    responses: {
      200: {
        content: { 'application/json': { schema: laneResponseSchema } },
        description: summary,
      },
      401: { content: errorContent, description: 'Unauthorized' },
      500: { content: errorContent, description: 'Server error' },
    },
  })
}

export function createSearchRoute(path: string, summary: string, tag: string) {
  return createRoute({
    method: 'get' as const,
    path,
    tags: [tag],
    summary,
    request: {
      query: z.object({
        query: z.string().openapi({ description: 'Search query', example: 'inception' }),
      }),
    },
    responses: {
      200: {
        content: { 'application/json': { schema: laneResponseSchema } },
        description: summary,
      },
      400: { content: errorContent, description: 'Bad request' },
      401: { content: errorContent, description: 'Unauthorized' },
      500: { content: errorContent, description: 'Server error' },
    },
  })
}

export function createDetailRoute(
  path: string,
  schema: z.ZodType,
  summary: string,
  tag: string,
  paramName = 'id'
) {
  return createRoute({
    method: 'get' as const,
    path,
    tags: [tag],
    summary,
    request: {
      params: z.object({
        [paramName]: z.string().openapi({ description: `${tag} ID` }),
      }),
    },
    responses: {
      200: { content: { 'application/json': { schema } }, description: summary },
      400: { content: errorContent, description: 'Bad request' },
      401: { content: errorContent, description: 'Unauthorized' },
      500: { content: errorContent, description: 'Server error' },
      501: { content: errorContent, description: 'Not supported' },
    },
  })
}

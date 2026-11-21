import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import type { ReactNode } from 'react'
import { vi } from 'vitest'
import type { JobDto } from '../api-dl'

export function makeJob(overrides: Partial<JobDto> = {}): JobDto {
  return {
    id: 1,
    name: 'Some.Show.S01E01.1080p.WEB-DL',
    kind: 'usenet',
    status: 'queued',
    priority: 0,
    total_bytes: 1024,
    added_at: 1_700_000_000,
    ...overrides,
  }
}

export function makeQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
}

export function queryWrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
  }
}

type Handler = (request: Request) => Response | Promise<Response>

/** Replaces fetch for one test and records every request the generated client sends. */
export function stubFetch(handler: Handler): Request[] {
  const requests: Request[] = []
  const fetchStub = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const request = new Request(input, init)
    requests.push(request)
    return handler(request)
  }
  vi.stubGlobal('fetch', fetchStub)
  return requests
}

export function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

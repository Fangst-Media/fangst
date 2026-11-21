import { type Protocol, isObject, str } from '../shared'

/** Which adapter the client handed `title` to, from the recording proxy's log: an NZB upload
 * or URL is usenet, a torrent upload or link is torrent. Fangst's own job list can't say, since
 * clients delete finished jobs after import. */
export async function handedOver(client: string, title: string): Promise<Protocol | undefined> {
  const file = Bun.file(`/records/${client}.jsonl`)
  if (!(await file.exists())) return undefined
  for (const line of (await file.text()).split('\n')) {
    if (line.trim() === '') continue
    let entry: unknown
    try {
      entry = JSON.parse(line)
    } catch {
      // The recorder may be halfway through writing the last line.
      continue
    }
    const request = isObject(entry) ? entry.request : undefined
    const sent = isObject(request) ? request.body : undefined
    // Torrent uploads are binary, so the log keeps them as base64.
    const base64 = str(sent, 'base64')
    const text = base64 === undefined ? line : Buffer.from(base64, 'base64').toString('latin1')
    if (!text.includes(title)) continue
    const path = str(request, 'path') ?? ''
    const mode = queryValue(request, 'mode')
    if (path === '/api/v2/torrents/add') return 'torrent'
    if (path === '/api' && (mode === 'addfile' || mode === 'addurl')) return 'usenet'
  }
  return undefined
}

function queryValue(request: unknown, name: string): string | undefined {
  const query = isObject(request) ? request.query : undefined
  if (!Array.isArray(query)) return undefined
  for (const pair of query) {
    if (Array.isArray(pair) && pair[0] === name && typeof pair[1] === 'string') return pair[1]
  }
  return undefined
}

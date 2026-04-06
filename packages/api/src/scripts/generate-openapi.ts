/**
 * Fetches the OpenAPI spec from the running API server and writes it to disk.
 * Usage: bun run src/scripts/generate-openapi.ts
 *
 * Requires the API server to be running on its configured port.
 */

const API_URL = process.env.API_URL ?? 'http://localhost:5001'

async function main() {
  console.log(`Fetching OpenAPI spec from ${API_URL}/api/openapi.json...`)

  const response = await fetch(`${API_URL}/api/openapi.json`)
  if (!response.ok) {
    console.error(`Failed to fetch spec: ${response.status} ${response.statusText}`)
    process.exit(1)
  }

  const spec = await response.json()
  const output = JSON.stringify(spec, null, 2)

  await Bun.write('./openapi.json', output)
  console.log(`Written to packages/api/openapi.json (${output.length} bytes)`)
}

main().catch((err) => {
  console.error('Failed:', err.message)
  process.exit(1)
})

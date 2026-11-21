/** Global setup hands the servers it started to the test workers through these variables. */
export const ENV = {
  apiKey: 'FANGST_E2E_API_KEY',
  daemonUrl: 'FANGST_E2E_DAEMON_URL',
  webUrl: 'FANGST_E2E_WEB_URL',
} as const

export function requireEnv(name: string): string {
  const value = process.env[name]
  if (value === undefined || value === '') {
    throw new Error(`${name} is not set; run the smoke test through \`playwright test\``)
  }
  return value
}

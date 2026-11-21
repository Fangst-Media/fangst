import { readFile } from 'node:fs/promises'
import path from 'node:path'
import { fileURLToPath } from 'node:url'
import { expect, test } from '@playwright/test'
import { ENV, requireEnv } from './env'

const nzbPath = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../../fixtures/nzb/password-two-files.nzb'
)

test('a job added through the API shows up live, then pauses and deletes from the UI', async ({
  page,
  request,
}) => {
  const apiKey = requireEnv(ENV.apiKey)
  const daemonUrl = requireEnv(ENV.daemonUrl)
  const webUrl = requireEnv(ENV.webUrl)
  const jobName = `Fangst.Smoke.${Date.now()}`

  await page.goto(`${webUrl}/downloads`)
  await page.getByLabel('API key').fill(apiKey)
  await page.getByRole('button', { name: 'Save key' }).click()

  await expect(page.getByTestId('events')).toHaveText('Live')
  await expect(page.getByText('The queue is empty.')).toBeVisible()

  // Added straight to the daemon, so only the event stream can bring it into the open page.
  const added = await request.post(`${daemonUrl}/api/v1/jobs`, {
    headers: { 'X-Api-Key': apiKey, 'Content-Type': 'application/x-nzb' },
    params: { name: jobName },
    data: await readFile(nzbPath),
  })
  expect(added.status()).toBe(201)

  const row = page.getByTestId('job-row').filter({ hasText: jobName })
  await expect(row).toBeVisible()
  await expect(row.getByText('Queued', { exact: true })).toBeVisible()

  await row.getByRole('button', { name: 'Pause' }).click()
  await expect(row.getByText('Paused', { exact: true })).toBeVisible()
  await expect(row.getByRole('button', { name: 'Resume' })).toBeVisible()

  await row.getByRole('button', { name: 'Delete' }).click()
  await row.getByRole('button', { name: 'Confirm delete' }).click()
  await expect(row).toHaveCount(0)
  await expect(page.getByText('The queue is empty.')).toBeVisible()

  const remaining = await request.get(`${daemonUrl}/api/v1/jobs`, {
    headers: { 'X-Api-Key': apiKey },
  })
  expect(remaining.status()).toBe(200)
  expect(await remaining.json()).toEqual([])
})

test('a magnet link added in the UI becomes a queued torrent job', async ({ page }) => {
  const apiKey = requireEnv(ENV.apiKey)
  const webUrl = requireEnv(ENV.webUrl)
  const jobName = `Fangst.Magnet.${Date.now()}`
  const hash = Date.now().toString(16).padStart(40, '0')

  await page.goto(`${webUrl}/downloads`)
  await page.getByLabel('API key').fill(apiKey)
  await page.getByRole('button', { name: 'Save key' }).click()
  await expect(page.getByTestId('events')).toHaveText('Live')

  await page.getByLabel('Magnet link').fill(`magnet:?xt=urn:btih:${hash}&dn=${jobName}`)
  await page.getByRole('button', { name: 'Add' }).click()

  // The smoke daemon runs without the torrent engine, so the job stays queued.
  const row = page.getByTestId('job-row').filter({ hasText: jobName })
  await expect(row).toBeVisible()
  await expect(row.getByText('torrent', { exact: true })).toBeVisible()
  await expect(row.getByText('Queued', { exact: true })).toBeVisible()
  await expect(page.getByLabel('Magnet link')).toHaveValue('')

  await row.getByRole('button', { name: 'Delete' }).click()
  await row.getByRole('button', { name: 'Confirm delete' }).click()
  await expect(row).toHaveCount(0)
})

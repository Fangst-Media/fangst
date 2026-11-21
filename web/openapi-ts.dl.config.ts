import { defineConfig } from '@hey-api/openapi-ts'

// The download daemon's spec is committed, so generation needs no running server.
export default defineConfig({
  input: '../openapi/dl.json',
  output: {
    path: 'src/api-dl',
  },
  plugins: ['@hey-api/client-fetch', '@hey-api/typescript', '@hey-api/sdk'],
})

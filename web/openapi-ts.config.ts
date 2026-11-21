import { defineConfig } from '@hey-api/openapi-ts'

export default defineConfig({
  input: 'http://localhost:5001/api/openapi.json',
  output: {
    path: 'src/api',
  },
  client: '@hey-api/client-fetch',
  plugins: [
    '@hey-api/typescript',
    {
      name: '@hey-api/sdk',
      asClass: true,
    },
  ],
})

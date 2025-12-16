export type ArrInstance = {
  id: string
  name: string
  type: 'sonarr' | 'radarr'
  url: string
  apiKey: string
  enabled: boolean
}

export type AppConfig = {
  arrInstances: ArrInstance[]
}

import { z } from 'zod'

const jellyfinAuthResponseSchema = z.object({
  User: z.object({
    Id: z.string(),
    Name: z.string(),
    Policy: z.object({
      IsAdministrator: z.boolean(),
    }),
  }),
  AccessToken: z.string(),
})

const jellyfinUserSchema = z.object({
  Id: z.string(),
  Name: z.string(),
  Policy: z.object({
    IsAdministrator: z.boolean(),
  }),
})

export type JellyfinUser = {
  id: string
  name: string
  isAdmin: boolean
  accessToken: string
}

export class JellyfinClient {
  private baseUrl: string
  private clientName = 'Fangst'
  private clientVersion = '0.1.0'
  private deviceId = 'fangst-server'
  private deviceName = 'Fangst Server'

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl.replace(/\/$/, '')
  }

  private getAuthHeader(token?: string): string {
    const parts = [
      `MediaBrowser Client="${this.clientName}"`,
      `Version="${this.clientVersion}"`,
      `DeviceId="${this.deviceId}"`,
      `Device="${this.deviceName}"`,
    ]
    if (token) {
      parts.push(`Token="${token}"`)
    }
    return parts.join(', ')
  }

  async authenticate(username: string, password: string): Promise<JellyfinUser> {
    const response = await fetch(`${this.baseUrl}/Users/AuthenticateByName`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Emby-Authorization': this.getAuthHeader(),
      },
      body: JSON.stringify({
        Username: username,
        Pw: password,
      }),
    })

    if (!response.ok) {
      if (response.status === 401) {
        throw new Error('Invalid username or password')
      }
      throw new Error(`Jellyfin authentication failed: ${response.statusText}`)
    }

    const data = await response.json()
    const parsed = jellyfinAuthResponseSchema.parse(data)

    return {
      id: parsed.User.Id,
      name: parsed.User.Name,
      isAdmin: parsed.User.Policy.IsAdministrator,
      accessToken: parsed.AccessToken,
    }
  }

  async validateToken(userId: string, token: string): Promise<JellyfinUser | null> {
    try {
      const response = await fetch(`${this.baseUrl}/Users/${userId}`, {
        headers: {
          'X-Emby-Authorization': this.getAuthHeader(token),
        },
      })

      if (!response.ok) {
        return null
      }

      const data = await response.json()
      const parsed = jellyfinUserSchema.parse(data)

      return {
        id: parsed.Id,
        name: parsed.Name,
        isAdmin: parsed.Policy.IsAdministrator,
        accessToken: token,
      }
    } catch {
      return null
    }
  }

  async logout(token: string): Promise<void> {
    await fetch(`${this.baseUrl}/Sessions/Logout`, {
      method: 'POST',
      headers: {
        'X-Emby-Authorization': this.getAuthHeader(token),
      },
    })
  }
}

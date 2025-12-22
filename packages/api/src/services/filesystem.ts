import { readdir, stat } from 'node:fs/promises'
import { readFileSync, existsSync } from 'node:fs'
import { join, extname } from 'node:path'

export type FileEntry = {
  path: string
  name: string
  isDirectory: boolean
  size?: number
}

export interface IFilesystem {
  readDirectory(path: string): Promise<FileEntry[]>
  isVideoFile(filename: string): boolean
}

const VIDEO_EXTENSIONS = ['.mkv', '.mp4', '.avi', '.m4v', '.ts']

export class RealFilesystem implements IFilesystem {
  async readDirectory(path: string): Promise<FileEntry[]> {
    const entries = await readdir(path, { withFileTypes: true })
    return Promise.all(
      entries.map(async (entry) => {
        const fullPath = join(path, entry.name)
        const isDir = entry.isDirectory()
        let size: number | undefined
        if (!isDir) {
          const stats = await stat(fullPath)
          size = stats.size
        }
        return {
          path: fullPath,
          name: entry.name,
          isDirectory: isDir,
          size,
        }
      })
    )
  }

  isVideoFile(filename: string): boolean {
    return VIDEO_EXTENSIONS.includes(extname(filename).toLowerCase())
  }
}

type MockFilesystemNode = {
  type: 'file' | 'directory'
  children?: Record<string, MockFilesystemNode>
}

type MockFilesystemConfig = {
  version: string
  description?: string
  root: string
  structure: Record<string, MockFilesystemNode>
}

export class MockFilesystem implements IFilesystem {
  private files: Map<string, FileEntry[]>
  private root: string

  constructor(jsonPath?: string) {
    this.files = new Map()
    this.root = '/media'

    const configPath = jsonPath || join(__dirname, '../tests/fixtures/mock-filesystem.json')

    if (existsSync(configPath)) {
      this.loadFromJson(configPath)
    } else {
      console.warn(`Mock filesystem config not found at ${configPath}, using empty structure`)
      this.files.set(this.root, [])
    }
  }

  private loadFromJson(jsonPath: string): void {
    try {
      const content = readFileSync(jsonPath, 'utf-8')
      const config: MockFilesystemConfig = JSON.parse(content)

      this.root = config.root
      this.files.set(this.root, [])

      for (const [name, node] of Object.entries(config.structure)) {
        this.traverseNode(this.root, name, node)
      }
    } catch (error) {
      console.error('Failed to load mock filesystem config:', error)
      this.files.set(this.root, [])
    }
  }

  private traverseNode(parentPath: string, nodeName: string, node: MockFilesystemNode): void {
    const currentPath = join(parentPath, nodeName)
    const isDirectory = node.type === 'directory'

    const parentEntries = this.files.get(parentPath) || []
    parentEntries.push({
      path: currentPath,
      name: nodeName,
      isDirectory,
    })
    this.files.set(parentPath, parentEntries)

    if (isDirectory && node.children) {
      this.files.set(currentPath, [])

      for (const [childName, childNode] of Object.entries(node.children)) {
        this.traverseNode(currentPath, childName, childNode)
      }
    }
  }

  async readDirectory(path: string): Promise<FileEntry[]> {
    const entries = this.files.get(path)
    if (!entries) {
      throw new Error(`Directory not found: ${path}`)
    }
    return entries
  }

  isVideoFile(filename: string): boolean {
    return VIDEO_EXTENSIONS.includes(extname(filename).toLowerCase())
  }
}

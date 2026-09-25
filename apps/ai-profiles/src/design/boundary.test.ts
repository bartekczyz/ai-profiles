import { readdir, readFile } from 'node:fs/promises'
import path from 'node:path'

import { describe, expect, it } from 'vitest'

const DESIGN_ROOT = path.resolve(__dirname)

/**
 * Import prefixes the design module may not use. Internal paths (`@/lib`, `@/components`, …) are
 * enforced by the `design` zone in `.fallowrc.json`; npm packages can't be fallow zones, so the
 * Tauri ban lives here.
 */
const forbiddenPrefixes = ['@tauri-apps/']

async function walk(directory: string): Promise<Array<string>> {
  const entries = await readdir(directory, { withFileTypes: true })
  const files: Array<string> = []
  for (const entry of entries) {
    const next = path.join(directory, entry.name)
    if (entry.isDirectory()) {
      const nested = await walk(next)
      for (const nestedPath of nested) {
        files.push(nestedPath)
      }
      continue
    }
    if (/\.(ts|tsx|css)$/.test(entry.name)) {
      files.push(next)
    }
  }
  return files
}

describe('design module boundary', () => {
  it('forbids imports from @tauri-apps packages', async () => {
    const files = await walk(DESIGN_ROOT)
    expect(files.length).toBeGreaterThan(0)
    const violations: Array<{ file: string; line: string }> = []
    for (const file of files) {
      const contents = await readFile(file, 'utf8')
      const lines = contents.split('\n')
      for (const line of lines) {
        if (!/\bfrom\b\s+['"]/.test(line) && !/^\s*import\s+['"]/.test(line)) {
          continue
        }
        for (const prefix of forbiddenPrefixes) {
          if (line.includes(`'${prefix}`) || line.includes(`"${prefix}`)) {
            violations.push({ file: path.relative(DESIGN_ROOT, file), line: line.trim() })
          }
        }
      }
    }
    expect(violations).toEqual([])
  })
})

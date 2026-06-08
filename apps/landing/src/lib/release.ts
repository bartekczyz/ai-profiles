export interface ReleaseAsset {
  name: string
  browser_download_url: string
  size: number
}

export interface Release {
  tag_name: string
  assets: ReadonlyArray<ReleaseAsset>
}

const repo = 'bartekczyz/ai-profiles'

export async function fetchLatest(): Promise<Release | null> {
  try {
    const response = await fetch(`https://api.github.com/repos/${repo}/releases/latest`, {
      headers: { Accept: 'application/vnd.github+json' },
    })
    if (!response.ok) {
      return null
    }
    return (await response.json()) as Release
  } catch {
    return null
  }
}

/**
 * Pick the most appropriate .dmg asset for the visitor's machine.
 * Heuristic, not perfect — Safari does not expose userAgentData.platform.
 * Falls back to "any .dmg" when arch cannot be determined.
 */
export function pickDmg(assets: ReadonlyArray<ReleaseAsset>): ReleaseAsset | null {
  const dmgs = assets.filter((asset) => /\.dmg$/i.test(asset.name))
  if (dmgs.length === 0) {
    return null
  }
  const aarch64 = dmgs.find((asset) => /aarch64|arm64/i.test(asset.name))
  const x64 = dmgs.find((asset) => /x64|x86_64|intel/i.test(asset.name))
  return aarch64 ?? x64 ?? dmgs[0]
}

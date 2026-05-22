import { useEffect, useState } from 'react'

import { getVersion } from '@tauri-apps/api/app'

import { useAppState } from '@/lib/app-state/use-app-state'

const FLASH_MS = 1600

type Props = {
  onOpenAbout: () => void
}

/**
 * Bottom row of the Settings pane.
 *
 * Left: mono `claude-profiles v{version} · MIT · Not affiliated with Anthropic`,
 * rendered as a button that opens the About dialog when clicked.
 * Right: ghost-styled "Reset onboarding flags →" button. Click clears
 * welcome/migration-dismissed/path-banner-dismissed via appState.update and
 * flashes a confirmation inline for FLASH_MS.
 */
export function SettingsFooterRow({ onOpenAbout }: Props) {
  const appState = useAppState()
  const [version, setVersion] = useState<string>('')
  const [flashedAt, setFlashedAt] = useState<number | null>(null)

  useEffect(() => {
    void getVersion().then(setVersion)
  }, [])

  useEffect(() => {
    if (flashedAt === null) {
      return
    }
    const handle = window.setTimeout(() => setFlashedAt(null), FLASH_MS)
    return () => window.clearTimeout(handle)
  }, [flashedAt])

  async function handleReset() {
    await appState.update({
      welcomeShown: false,
      clearMigrationDismissed: true,
      clearPathBannerDismissed: true,
    })
    setFlashedAt(Date.now())
  }

  return (
    <div className="mt-9 flex items-center justify-between gap-4 border-t border-border-soft pt-4">
      <button
        type="button"
        onClick={onOpenAbout}
        title="About claude-profiles"
        className="cursor-pointer rounded-md px-2 py-1 font-mono text-[10.5px] text-muted-strong transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-cream-2 hover:text-ink dark:hover:bg-white/[0.04]"
      >
        claude-profiles {version ? `v${version}` : '…'} · MIT · Not affiliated with Anthropic
      </button>
      <div className="flex items-center gap-2">
        {flashedAt !== null ? (
          <span className="font-mono text-[11px] text-muted-strong" role="status" aria-live="polite">
            Reset. Restart to see the welcome flow.
          </span>
        ) : null}
        <button
          type="button"
          onClick={() => void handleReset()}
          className="cursor-pointer rounded-md px-2 py-1 text-[11.5px] text-muted transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-cream-2 hover:text-ink dark:hover:bg-white/[0.04]"
        >
          Reset onboarding flags →
        </button>
      </div>
    </div>
  )
}

import type { Shell } from '@/lib/types'

import { useState } from 'react'

import { Button } from '@/design/ui/button'
import { detectShell, installPathHook } from '@/lib/commands'

type Props = {
  onFixed: () => void
  onDismiss: () => void
}

const RC_DISPLAY_NAMES: Record<Shell, string> = {
  zsh: '~/.zshrc',
  bash: '~/.bashrc',
  fish: '~/.config/fish/config.fish',
}

export function PathSetupBanner({ onFixed, onDismiss }: Props) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<string | null>(null)

  async function handleFix() {
    setBusy(true)
    setError(null)
    setSuccess(null)
    try {
      const shell = await detectShell()
      const outcome = await installPathHook(shell)
      if (outcome.outcome === 'alreadyInstalled') {
        setSuccess(`${RC_DISPLAY_NAMES[shell]} already has the line — open a new terminal to pick it up.`)
      } else {
        setSuccess(`Updated ${RC_DISPLAY_NAMES[shell]}. Open a new terminal to pick up the change.`)
      }
      onFixed()
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="mx-6 mt-4 rounded-md border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-900">
      <div className="flex items-start justify-between gap-4">
        <div>
          <p className="font-semibold">
            Your shell doesn&apos;t look in <code>~/.local/bin</code>
          </p>
          <p className="mt-1 text-xs">
            CLI wrappers are installed there. Typing <code>claude-personal</code> won&apos;t work until your shell PATH
            includes that directory.
          </p>
          {success ? <p className="mt-2 text-xs">{success}</p> : null}
          {error ? <p className="mt-2 text-xs text-red">{error}</p> : null}
        </div>
        <div className="flex shrink-0 gap-2">
          <Button size="sm" onClick={handleFix} disabled={busy}>
            Fix it for me
          </Button>
          <Button size="sm" variant="ghost" onClick={onDismiss} disabled={busy}>
            Not now
          </Button>
        </div>
      </div>
    </div>
  )
}

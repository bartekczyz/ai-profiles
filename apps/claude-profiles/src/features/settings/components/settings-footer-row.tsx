import { useEffect, useState } from 'react'

import { getVersion } from '@tauri-apps/api/app'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { useAppState } from '@/lib/app-state/use-app-state'

type Props = {
  onOpenAbout: () => void
}

/**
 * Bottom row of the Settings pane.
 *
 * Left: mono `claude-profiles v{version} · MIT · Not affiliated with Anthropic or OpenAI`,
 * rendered as a button that opens the About dialog when clicked.
 * Right: ghost-styled "Reset onboarding flags →" button. Click opens a
 * confirmation dialog; confirming clears welcome/migration-dismissed/
 * path-banner-dismissed via appState.update and surfaces a toast.
 */
export function SettingsFooterRow({ onOpenAbout }: Props) {
  const appState = useAppState()
  const toast = useToast()
  const [version, setVersion] = useState<string>('')
  const [confirmOpen, setConfirmOpen] = useState(false)
  const [submitting, setSubmitting] = useState(false)

  useEffect(() => {
    void getVersion().then(setVersion)
  }, [])

  async function handleConfirm() {
    if (submitting) {
      return
    }
    setSubmitting(true)
    try {
      await appState.update({
        welcomeShown: false,
        clearMigrationDismissed: true,
        clearPathBannerDismissed: true,
      })
      toast.success('Onboarding flags reset.', 'Restart to see the welcome flow.')
      setConfirmOpen(false)
    } catch (caught) {
      toast.error("Couldn't reset onboarding flags.", caught instanceof Error ? caught.message : String(caught))
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <>
      <div className="mt-9 flex items-center justify-between gap-4 border-t border-border-soft pt-4">
        <button
          type="button"
          onClick={onOpenAbout}
          title="About claude-profiles"
          className="cursor-pointer rounded-md px-2 py-1 font-mono text-[10.5px] text-muted-strong transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-cream-2 hover:text-ink dark:hover:bg-white/[0.04]"
        >
          claude-profiles {version ? `v${version}` : '…'} · MIT · Not affiliated with Anthropic or OpenAI
        </button>
        <button
          type="button"
          onClick={() => setConfirmOpen(true)}
          className="cursor-pointer rounded-md px-2 py-1 text-[11.5px] text-muted transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-cream-2 hover:text-ink dark:hover:bg-white/[0.04]"
        >
          Reset onboarding flags →
        </button>
      </div>

      <Dialog
        open={confirmOpen}
        title="Reset onboarding state?"
        description="The welcome dialog and the existing-Claude import banner will appear again on next launch. Profiles, backups, and theme stay untouched."
        onClose={() => setConfirmOpen(false)}
        onSubmit={handleConfirm}
        foot={
          <>
            <Button
              disabled={submitting}
              variant="ghost"
              size="sm"
              trailingKbd={<Kbd>⎋</Kbd>}
              onClick={() => setConfirmOpen(false)}
            >
              Cancel
            </Button>
            <Button
              disabled={submitting}
              variant="primary"
              size="sm"
              trailingKbd={<Kbd variant="onOrange">⏎</Kbd>}
              onClick={handleConfirm}
            >
              Reset
            </Button>
          </>
        }
      >
        <p className="text-body text-ink-soft">This clears three flags on the local app state:</p>
        <ul className="mt-2 space-y-1 pl-5 text-body text-muted">
          <li className="list-disc">
            <code className="font-mono text-mono">welcomeShown</code>
          </li>
          <li className="list-disc">
            <code className="font-mono text-mono">migrationDismissedAt</code>
          </li>
          <li className="list-disc">
            <code className="font-mono text-mono">pathBannerDismissedAt</code>
          </li>
        </ul>
      </Dialog>
    </>
  )
}

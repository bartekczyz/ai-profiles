import { useEffect, useRef } from 'react'

import { useToast } from '@/design'

import { useWhatsNew } from '../api/use-whats-new'
import { WhatsNewDialog } from './whats-new-dialog'

// Long enough to reach the action; the 5 s default is too short, and a persistent toast would
// stack with the "update available" one.
const toastDurationMs = 15000

type WhatsNewHostProps = {
  /**
   * Whether the release-notes dialog is open. Owned by the parent so other entry points (About)
   * can open the same dialog.
   */
  open: boolean
  /**
   * Called when the upgrade toast's "What's new" action is pressed.
   */
  onOpen: () => void
  /**
   * Called when the dialog is dismissed.
   */
  onClose: () => void
}

/**
 * Mounts at the app root under `Suspense`. After an upgrade it shows a one-time toast whose action
 * opens the release-notes dialog, and renders that dialog. The ref guard keeps a re-render (or
 * StrictMode's double effect) from toasting the same version twice.
 */
export function WhatsNewHost({ open, onOpen, onClose }: WhatsNewHostProps) {
  const { version, upgraded, releases } = useWhatsNew()
  const toast = useToast()
  const toastedVersionRef = useRef<string | null>(null)

  useEffect(() => {
    if (!upgraded || toastedVersionRef.current === version) {
      return
    }
    toastedVersionRef.current = version
    toast.show({
      tone: 'info',
      title: `Updated to v${version}`,
      description: 'See what changed in this release.',
      durationMs: toastDurationMs,
      action: { label: "What's new", onClick: onOpen },
    })
  }, [upgraded, version, toast, onOpen])

  return <WhatsNewDialog open={open} releases={releases} onClose={onClose} />
}

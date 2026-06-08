import type { Profile } from '@/lib/types'

import { useState } from 'react'

import { Button, Dialog, Kbd } from '@/design'
import { appSpecs, wrapperFileName } from '@/lib/app-registry'

type Props = {
  open: boolean
  profile: Profile
  onClose: () => void
  onConfirm: (input: { moveToTrash: boolean }) => Promise<void>
}

export function DeleteProfileDialog({ open, profile, onClose, onConfirm }: Props) {
  const [moveToTrash, setMoveToTrash] = useState(true)
  const [submitting, setSubmitting] = useState(false)

  async function handleConfirm() {
    setSubmitting(true)
    try {
      await onConfirm({ moveToTrash })
      onClose()
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog
      open={open}
      title={`Delete ${profile.name}?`}
      description="The launchers will go, and the data directory follows the option below."
      onClose={onClose}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={submitting} onClick={onClose}>
            Cancel
          </Button>
          <Button variant="danger" size="sm" trailingKbd={<Kbd>⌘⌫</Kbd>} disabled={submitting} onClick={handleConfirm}>
            Delete
          </Button>
        </>
      }
    >
      <div className="space-y-3">
        <p className="text-body text-ink-soft">This will remove:</p>
        <ul className="space-y-1 pl-5 text-body text-muted">
          <li className="list-disc">The profile entry from ai-profiles</li>
          {profile.surfaces.gui ? (
            <li className="list-disc">
              <code className="font-mono text-mono">
                /Applications/{appSpecs[profile.app].displayName} ({profile.name}).app
              </code>
            </li>
          ) : null}
          {profile.surfaces.cli ? (
            <li className="list-disc">
              <code className="font-mono text-mono">{wrapperFileName(profile.app, profile.slug)}</code>
            </li>
          ) : null}
          <li className="list-disc">The per-profile data directory (history, cache, settings)</li>
        </ul>
        <label className="flex cursor-pointer items-center gap-2 pt-2 text-body text-ink-soft">
          <input
            type="checkbox"
            checked={moveToTrash}
            onChange={(event) => setMoveToTrash(event.target.checked)}
            className="h-4 w-4 cursor-pointer accent-orange"
          />
          Move the data directory to Trash instead of deleting permanently
        </label>
      </div>
    </Dialog>
  )
}

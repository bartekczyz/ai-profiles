import type { Profile } from '@/lib/types'

import { useState } from 'react'

import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/design/ui/alert-dialog'

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
    <AlertDialog open={open} onOpenChange={(next) => (next ? null : onClose())}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Delete &ldquo;{profile.name}&rdquo;?</AlertDialogTitle>
          <AlertDialogDescription asChild>
            <div>
              <p>This will remove:</p>
              <ul className="mt-2 list-disc pl-5 text-sm">
                <li>The profile entry from claude-profiles</li>
                {profile.surfaces.gui ? (
                  <li>
                    <code>/Applications/Claude ({profile.name}).app</code>
                  </li>
                ) : null}
                {profile.surfaces.cli ? (
                  <li>
                    <code>~/.local/bin/claude-{profile.slug}</code>
                  </li>
                ) : null}
                <li>The per-profile data directory (history, cache, settings)</li>
              </ul>
              <label className="mt-3 flex items-center gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={moveToTrash}
                  onChange={(event) => setMoveToTrash(event.target.checked)}
                />
                Move the data directory to Trash instead of deleting permanently
              </label>
            </div>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={submitting}>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={handleConfirm} disabled={submitting}>
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  )
}

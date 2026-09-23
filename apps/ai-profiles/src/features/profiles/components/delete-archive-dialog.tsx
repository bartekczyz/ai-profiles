import type { ArchivedSession } from '@/lib/types'

import { useState } from 'react'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { formatBytes } from '@/lib/format-bytes'

import { useDeleteArchivedSession } from '../api/use-profile-sessions'
import { sessionErrorMessage } from './session-error-message'

type Props = {
  /** Profile id, or `default:claude`. */
  profileId: string
  session: ArchivedSession
  onClose: () => void
}

/**
 * Asks before deleting an archived session for good, saying what that frees:
 * unlike archiving, it can't be undone.
 */
export function DeleteArchiveDialog({ profileId, session, onClose }: Props) {
  const remove = useDeleteArchivedSession()
  const toast = useToast()
  const [error, setError] = useState<string | null>(null)
  const size = formatBytes(session.sizeBytes)

  async function handleDelete() {
    if (remove.isPending) {
      return
    }
    setError(null)
    try {
      const freed = await remove.mutateAsync({ profileId, sessionId: session.id, archive: session.archive })
      onClose()
      toast.success(`Deleted, freeing ${formatBytes(freed)}`, session.title ?? session.id)
    } catch (caught) {
      setError(sessionErrorMessage(caught, 'The archive could not be deleted.'))
    }
  }

  return (
    <Dialog
      open
      title="Delete this archive?"
      description={session.title ?? session.id}
      onClose={onClose}
      onSubmit={handleDelete}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={remove.isPending} onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="danger"
            size="sm"
            trailingKbd={<Kbd>⏎</Kbd>}
            disabled={remove.isPending}
            onClick={handleDelete}
          >
            {remove.isPending ? 'Deleting…' : `Delete, freeing ${size}`}
          </Button>
        </>
      }
    >
      <div className="space-y-2 text-body text-ink-soft">
        <p>
          The archived transcript{session.inDesktop ? ' and its desktop app record go' : ' goes'} for good, and the
          session can't be restored. This frees {size}.
        </p>
        {error ? (
          <p role="alert" className="text-meta text-red">
            {error}
          </p>
        ) : null}
      </div>
    </Dialog>
  )
}

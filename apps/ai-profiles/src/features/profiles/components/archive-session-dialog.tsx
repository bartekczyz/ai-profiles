import type { SessionSummary } from '@/lib/types'

import { useState } from 'react'

import { Button, Dialog, Kbd } from '@/design'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { useArchiveSession } from '../api/use-profile-sessions'

type Props = {
  open: boolean
  /** Profile id the session is in, or `default:claude`. */
  profileId: string
  session: SessionSummary
  onClose: () => void
}

/**
 * Takes a session out of this profile. Nothing is deleted: the transcript, and
 * the desktop app's record of it, go to session-transfer-backups. What stops
 * it (the app that lists the session is running) comes back as an error and
 * is shown in place, so the user can quit that app and press Archive again.
 */
export function ArchiveSessionDialog({ open, profileId, session, onClose }: Props) {
  const archive = useArchiveSession()
  const [error, setError] = useState<string | null>(null)
  const title = session.title ?? session.lastPrompt ?? session.id

  async function handleArchive() {
    if (archive.isPending) {
      return
    }
    setError(null)
    try {
      await archive.mutateAsync({ profileId, sessionId: session.id })
      onClose()
    } catch (caught) {
      setError(extractErrorMessage(caught, 'The session could not be archived.'))
    }
  }

  return (
    <Dialog
      open={open}
      title="Archive session?"
      description={title}
      onClose={onClose}
      onSubmit={handleArchive}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={archive.isPending} onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="sm"
            trailingKbd={<Kbd>⏎</Kbd>}
            disabled={archive.isPending}
            onClick={handleArchive}
          >
            {archive.isPending ? 'Archiving…' : 'Archive'}
          </Button>
        </>
      }
    >
      <div className="space-y-2 text-body text-ink-soft">
        <p>
          It leaves this profile's session list
          {session.inDesktop ? ', and its desktop app' : ''}. Nothing is deleted: the transcript is kept in{' '}
          <code className="font-mono text-mono">session-transfer-backups</code>, so it can be put back.
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

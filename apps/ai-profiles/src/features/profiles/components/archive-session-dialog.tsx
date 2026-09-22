import type { SessionSummary } from '@/lib/types'

import { useState } from 'react'

import { Button, Dialog, Kbd } from '@/design'

import { useArchiveCheck, useArchiveSession } from '../api/use-profile-sessions'
import { appsToQuitNote } from './apps-to-quit'
import { sessionErrorMessage } from './session-error-message'

type Props = {
  open: boolean
  /** Profile id the session is in, or `default:claude`. */
  profileId: string
  session: SessionSummary
  onClose: () => void
}

/**
 * Takes a session out of this profile. Nothing is deleted: the transcript, and
 * the desktop app's record of it, go to session-transfer-backups. When the
 * profile's desktop app has the session open or lists it, the button says it
 * will quit that app first, and does; a terminal holding the session is
 * something only the user can close, so that is shown and the button held.
 */
export function ArchiveSessionDialog({ open, profileId, session, onClose }: Props) {
  const archive = useArchiveSession()
  const check = useArchiveCheck(profileId, session.id)
  const [error, setError] = useState<string | null>(null)
  const appToQuit = check.data?.appToQuit ?? null
  const blocker = check.data?.blocker ?? (check.error ? sessionErrorMessage(check.error) : null)
  const ready = check.data !== undefined && blocker === null && !archive.isPending
  const title = session.title ?? session.lastPrompt ?? session.id

  async function handleArchive() {
    if (!ready) {
      return
    }
    setError(null)
    try {
      await archive.mutateAsync({ profileId, sessionId: session.id, quitApp: appToQuit !== null })
      onClose()
    } catch (caught) {
      setError(sessionErrorMessage(caught, 'The session could not be archived.'))
      await check.refetch()
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
          <Button variant="primary" size="sm" trailingKbd={<Kbd>⏎</Kbd>} disabled={!ready} onClick={handleArchive}>
            {archive.isPending
              ? appToQuit
                ? 'Quitting and archiving…'
                : 'Archiving…'
              : appToQuit
                ? 'Quit and archive'
                : 'Archive'}
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
        {blocker ? (
          <p role="alert" className="text-meta text-red">
            {blocker}
          </p>
        ) : appToQuit ? (
          <p className="text-meta text-amber">{appsToQuitNote([appToQuit])}</p>
        ) : null}
        {error ? (
          <p role="alert" className="text-meta text-red">
            {error}
          </p>
        ) : null}
      </div>
    </Dialog>
  )
}

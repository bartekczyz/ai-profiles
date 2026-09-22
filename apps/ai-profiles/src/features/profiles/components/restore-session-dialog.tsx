import type { ArchivedSession } from '@/lib/types'

import { useState } from 'react'

import { Button, Dialog, Kbd } from '@/design'

import { useRestoreCheck, useRestoreSession } from '../api/use-profile-sessions'
import { appsToQuitNote } from './apps-to-quit'
import { sessionErrorMessage } from './session-error-message'

type Props = {
  open: boolean
  /** Profile id the session was archived from, or `default:claude`. */
  profileId: string
  session: ArchivedSession
  onClose: () => void
}

/**
 * Puts an archived session back where it was: its transcript into the
 * profile's session list, and its desktop record, if it had one, into the
 * app's. Like archiving, it quits that app first when the record has to go
 * back into its list, and says so on the button.
 */
export function RestoreSessionDialog({ open, profileId, session, onClose }: Props) {
  const restore = useRestoreSession()
  const check = useRestoreCheck(profileId, session.id, session.archive)
  const [error, setError] = useState<string | null>(null)
  const appToQuit = check.data?.appToQuit ?? null
  const blocker = check.data?.blocker ?? (check.error ? sessionErrorMessage(check.error) : null)
  const ready = check.data !== undefined && blocker === null && !restore.isPending
  const title = session.title ?? session.id

  async function handleRestore() {
    if (!ready) {
      return
    }
    setError(null)
    try {
      await restore.mutateAsync({
        profileId,
        sessionId: session.id,
        archive: session.archive,
        quitApp: appToQuit !== null,
      })
      onClose()
    } catch (caught) {
      setError(sessionErrorMessage(caught, 'The session could not be restored.'))
      await check.refetch()
    }
  }

  return (
    <Dialog
      open={open}
      title="Restore session?"
      description={title}
      onClose={onClose}
      onSubmit={handleRestore}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={restore.isPending} onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" trailingKbd={<Kbd>⏎</Kbd>} disabled={!ready} onClick={handleRestore}>
            {restore.isPending
              ? appToQuit
                ? 'Quitting and restoring…'
                : 'Restoring…'
              : appToQuit
                ? 'Quit and restore'
                : 'Restore'}
          </Button>
        </>
      }
    >
      <div className="space-y-2 text-body text-ink-soft">
        <p>
          It goes back into this profile's session list
          {session.inDesktop ? ', and its desktop app' : ''}, where it was before it was archived.
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

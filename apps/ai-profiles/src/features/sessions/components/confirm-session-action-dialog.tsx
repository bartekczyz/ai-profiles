import type { ActionCheck, Session, SessionAction } from '@/lib/types'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { shortenHomePath } from '@/features/profiles/components/shorten-home-path'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { useSessionAction, useSessionActionCheck } from '../api/use-session-actions'
import { untitledSessionLabel } from './session-row'

type Props = {
  /**
   * The profile whose session it is — a managed profile's id, or
   * `default:<app>`.
   */
  profileId: string
  /**
   * The session to act on.
   */
  session: Session
  /**
   * What to do to it.
   */
  action: SessionAction
  /**
   * Closes the dialog: once the action is done, or when it is called off.
   */
  onClose: () => void
}

type NoticeProps = {
  /**
   * What the action's check found, once it has landed.
   */
  check: ActionCheck | undefined
  /**
   * Why the check, or the action itself, failed.
   */
  errorMessage: string | null
  /**
   * The folder the session works in.
   */
  cwd: string | null
}

export type PrimaryProps = {
  /**
   * Whether the action can't go ahead yet: the check is on its way, it
   * failed, or the action is under way.
   */
  disabled: boolean
  /**
   * The primary button's text.
   */
  label: string
  /**
   * Why only the user can let the action go ahead, if that is so.
   */
  blocker: string | null
  /**
   * Does the action.
   */
  onConfirm: () => void
}

/**
 * How each action is named on its button and in its title.
 */
const verbs: Record<SessionAction, string> = {
  archive: 'Archive',
  restore: 'Restore',
}

/**
 * What each action does to the session, in a line under the title.
 */
const consequences: Record<SessionAction, string> = {
  archive: 'It moves to Archived. Restore it any time.',
  restore: 'It moves back to Active.',
}

/**
 * What the toast says once each action is done.
 */
const doneMessages: Record<SessionAction, string> = {
  archive: 'Session archived',
  restore: 'Session restored',
}

/**
 * Asks before archiving or restoring a session, after checking what stands in
 * the way. Something only the user can clear, like a terminal that has the
 * session open, takes the primary button's place. A desktop app that has to
 * quit first is named on the button, which quits it on the way. The dialog
 * closes once the action is done and stays open, saying why, if it fails.
 */
export function ConfirmSessionActionDialog({ profileId, session, action, onClose }: Props) {
  const checkQuery = useSessionActionCheck(profileId, session.id, action)
  const sessionAction = useSessionAction(profileId)
  const toast = useToast()

  const check = checkQuery.data
  const appToQuit = check?.appToQuit ?? null
  const blocker = check?.blocker ?? null
  const verb = verbs[action]
  const primaryLabel = appToQuit === null ? verb : `Quit ${appToQuit.label} and ${verb.toLowerCase()}`
  const canConfirm = check !== undefined && blocker === null && !sessionAction.isPending
  const failure = sessionAction.error ?? (checkQuery.isError ? checkQuery.error : null)
  const errorMessage = failure === null ? null : extractErrorMessage(failure)

  function handleConfirm() {
    if (!canConfirm) {
      return
    }
    sessionAction.mutate(
      { sessionId: session.id, action, quitApp: appToQuit !== null },
      {
        onSuccess: () => {
          toast.success(doneMessages[action], session.title ?? untitledSessionLabel)
          onClose()
        },
      },
    )
  }

  return (
    <Dialog
      open
      title={`${verb} "${session.title ?? untitledSessionLabel}"?`}
      description={consequences[action]}
      onClose={onClose}
      onSubmit={handleConfirm}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} onClick={onClose}>
            Cancel
          </Button>
          <PrimaryAction disabled={!canConfirm} label={primaryLabel} blocker={blocker} onConfirm={handleConfirm} />
        </>
      }
    >
      <ActionNotice check={check} errorMessage={errorMessage} cwd={session.cwd} />
    </Dialog>
  )
}

/**
 * The dialog's body: why the action failed, else that the check is still
 * looking, else the desktop app that quits first, else the session's folder.
 */
function ActionNotice({ check, errorMessage, cwd }: NoticeProps) {
  if (errorMessage !== null) {
    return (
      <p role="alert" className="text-body text-red">
        {errorMessage}
      </p>
    )
  }
  if (check === undefined) {
    return <p className="text-body text-muted">Checking what the session is doing…</p>
  }
  if (check.appToQuit !== null) {
    return (
      <p className="text-body text-ink-soft">
        {check.appToQuit.label} keeps a record of this session open, so it quits first. Open it again afterwards.
      </p>
    )
  }
  if (cwd === null) {
    return null
  }
  return (
    <p title={cwd} className="truncate font-mono text-mono text-muted-strong">
      {shortenHomePath(cwd)}
    </p>
  )
}

/**
 * The primary button, or, when only the user can let the action go ahead,
 * the reason in its place.
 */
export function PrimaryAction({ disabled, label, blocker, onConfirm }: PrimaryProps) {
  if (blocker !== null) {
    return <p className="max-w-[60%] text-right text-meta text-muted-strong">{blocker}</p>
  }
  return (
    <Button disabled={disabled} variant="primary" size="sm" trailingKbd={<Kbd>↵</Kbd>} onClick={onConfirm}>
      {label}
    </Button>
  )
}

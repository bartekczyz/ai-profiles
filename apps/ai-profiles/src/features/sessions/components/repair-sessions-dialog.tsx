import type { ActionCheck } from '@/lib/types'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { useRepairSessions, useSessionRepairCheck } from '../api/use-session-actions'
import { repairToast } from '../lib/repair-toast'
import { sessionCount } from '../lib/session-count'
import { PrimaryAction } from './confirm-session-action-dialog'

type Props = {
  /**
   * The profile whose sessions need repair — a managed profile's id, or
   * `default:<app>`.
   */
  profileId: string
  /**
   * The profile's name.
   */
  profileLabel: string
  /**
   * How many of its sessions need repair.
   */
  repairCount: number
  /**
   * Closes the dialog: once the repair is done, or when it is called off.
   */
  onClose: () => void
}

type NoticeProps = {
  /**
   * What the repair's check found, once it has landed.
   */
  check: ActionCheck | undefined
  /**
   * Why the check, or the repair itself, failed.
   */
  errorMessage: string | null
}

/**
 * Asks before repairing a profile's sessions that its desktop app started
 * before the profile had its own folder, saying what repair does. The
 * profile's desktop app, when it runs, is named on the button, which quits it
 * on the way. The dialog closes once the repair is done, with a toast saying
 * how many sessions were repaired and skipped and why, and stays open, saying
 * why, if it fails.
 */
export function RepairSessionsDialog({ profileId, profileLabel, repairCount, onClose }: Props) {
  const checkQuery = useSessionRepairCheck(profileId)
  const repair = useRepairSessions(profileId)
  const toast = useToast()

  const check = checkQuery.data
  const appToQuit = check?.appToQuit ?? null
  const blocker = check?.blocker ?? null
  const primaryLabel = appToQuit === null ? 'Repair' : `Quit ${appToQuit.label} and repair`
  const canConfirm = check !== undefined && blocker === null && !repair.isPending
  const failure = repair.error ?? (checkQuery.isError ? checkQuery.error : null)
  const errorMessage = failure === null ? null : extractErrorMessage(failure)

  function handleConfirm() {
    if (!canConfirm) {
      return
    }
    repair.mutate(appToQuit !== null, {
      onSuccess: (report) => {
        const summary = repairToast(report, profileLabel)
        toast[summary.tone](summary.title, summary.description)
        onClose()
      },
      // What stood in the way may have changed on the way (the app quit, or
      // started again), so a retry goes by a fresh check.
      onError: () => {
        void checkQuery.refetch()
      },
    })
  }

  return (
    <Dialog
      open
      title={`Repair ${sessionCount(repairCount)}?`}
      description={`These sessions were started in ${profileLabel}'s desktop app before it had its own folder. Repair moves them into it so the app can open them.`}
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
      <RepairNotice check={check} errorMessage={errorMessage} />
    </Dialog>
  )
}

/**
 * The dialog's body: why the repair failed, else that the check is still
 * looking, else the desktop app that quits first.
 */
function RepairNotice({ check, errorMessage }: NoticeProps) {
  if (errorMessage !== null) {
    return (
      <p role="alert" className="text-body text-red">
        {errorMessage}
      </p>
    )
  }
  if (check === undefined) {
    return <p className="text-body text-muted">Checking what the sessions are doing…</p>
  }
  if (check.appToQuit !== null) {
    return (
      <p className="text-body text-ink-soft">
        {check.appToQuit.label} has these sessions' folder open, so it quits first. Open it again afterwards.
      </p>
    )
  }
  return null
}

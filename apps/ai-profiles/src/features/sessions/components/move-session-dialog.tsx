import type { MovePlan, Session } from '@/lib/types'

import { useState } from 'react'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { extractErrorKind, extractErrorMessage } from '@/lib/extract-error-message'

import { useMoveSession, useSessionMovePlan } from '../api/use-session-actions'
import { PrimaryAction } from './confirm-session-action-dialog'
import { untitledSessionLabel } from './session-row'

/**
 * A profile a session can be moved to.
 */
export type MoveTarget = {
  /**
   * The profile's id, or `default:<app>`.
   */
  id: string
  /**
   * The profile's display name.
   */
  label: string
}

type Props = {
  /**
   * The profile the session is in — a managed profile's id, or
   * `default:<app>`.
   */
  profileId: string
  /**
   * The session to move.
   */
  session: Session
  /**
   * Where to move it.
   */
  destination: MoveTarget
  /**
   * Closes the dialog: once the session has moved, or when it is called off.
   */
  onClose: () => void
}

type PlanBodyProps = {
  /**
   * Whether the user agreed to replace a newer copy at the destination.
   */
  replaceNewer: boolean
  /**
   * The plan, once it has landed.
   */
  plan: MovePlan | undefined
  /**
   * What the move needs installed first, when that is why the plan or the
   * move failed.
   */
  unavailableMessage: string | null
  /**
   * Why the plan, or the move itself, failed otherwise.
   */
  errorMessage: string | null
  /**
   * Where the session goes.
   */
  destinationLabel: string
  /**
   * Records the user's answer to replacing a newer copy.
   */
  onReplaceNewerChange: (replaceNewer: boolean) => void
}

type PlanDetailsProps = {
  /**
   * The plan to show.
   */
  plan: MovePlan
  /**
   * Whether the user agreed to replace a newer copy at the destination.
   */
  replaceNewer: boolean
  /**
   * Where the session goes.
   */
  destinationLabel: string
  /**
   * Records the user's answer to replacing a newer copy.
   */
  onReplaceNewerChange: (replaceNewer: boolean) => void
}

/**
 * How each item action reads in the file list.
 */
const itemActionLabels: Record<MovePlan['items'][number]['action'], string> = {
  copy: 'copy',
  same: 'already there',
  replace: 'replace',
}

/**
 * Moves a session to another profile after showing what the move would do:
 * one summary line, what the user should know, and the files it copies,
 * folded away. A newer copy at the destination is only replaced once the user
 * ticks the box; desktop apps in the way are named on the button, which quits
 * them on the way. Something only the user can clear takes the button's
 * place. The dialog closes once the session has moved and stays open, saying
 * why, if the move fails, with its plan looked up again for a retry.
 */
export function MoveSessionDialog({ profileId, session, destination, onClose }: Props) {
  const planQuery = useSessionMovePlan(profileId, session.id, destination.id)
  const move = useMoveSession(profileId)
  const toast = useToast()
  const [replaceNewer, setReplaceNewer] = useState(false)

  const plan = planQuery.data
  const appsToQuit = plan?.appsToQuit ?? []
  const blocker = plan === undefined || plan.blockers.length === 0 ? null : plan.blockers.join('. ')
  const primaryLabel =
    appsToQuit.length === 0 ? 'Move' : `Quit ${appsToQuit.map((app) => app.label).join(' and ')} and move`
  const canConfirm =
    plan !== undefined && blocker === null && (!plan.destinationNewer || replaceNewer) && !move.isPending
  const failure = move.error ?? (planQuery.isError ? planQuery.error : null)
  // A missing tool is a state to explain, not a failure to raise the alarm over.
  const missingTool = failure !== null && extractErrorKind(failure) === 'NotInstalled'
  const unavailableMessage = missingTool ? extractErrorMessage(failure) : null
  const errorMessage = failure !== null && !missingTool ? extractErrorMessage(failure) : null
  const title = session.title ?? untitledSessionLabel

  function handleConfirm() {
    if (!canConfirm) {
      return
    }
    move.mutate(
      { sessionId: session.id, destinationId: destination.id, replaceNewer, quitApps: appsToQuit.length > 0 },
      {
        onSuccess: (report) => {
          const conflicts = report.memoryConflicts
          const description =
            conflicts.length === 0 ? title : `${destination.label} kept its own memory of ${conflicts.join(', ')}`
          toast.success(`Moved to ${destination.label}`, description)
          onClose()
        },
        // What stood in the way may have changed on the way (an app quit, or
        // started again), so a retry goes by a fresh plan.
        onError: () => {
          void planQuery.refetch()
        },
      },
    )
  }

  return (
    <Dialog
      open
      title={`Move "${title}" to ${destination.label}?`}
      description="It's archived here afterwards. Restore it here any time."
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
      <PlanBody
        replaceNewer={replaceNewer}
        plan={plan}
        unavailableMessage={unavailableMessage}
        errorMessage={errorMessage}
        destinationLabel={destination.label}
        onReplaceNewerChange={setReplaceNewer}
      />
    </Dialog>
  )
}

/**
 * The dialog's body: what the move needs installed, else why the plan or the
 * move failed, else that the plan is still on its way, else the plan.
 */
function PlanBody({
  replaceNewer,
  plan,
  unavailableMessage,
  errorMessage,
  destinationLabel,
  onReplaceNewerChange,
}: PlanBodyProps) {
  if (unavailableMessage !== null) {
    return <p className="text-body text-ink-soft">{unavailableMessage}</p>
  }
  if (errorMessage !== null) {
    return (
      <p role="alert" className="text-body text-red">
        {errorMessage}
      </p>
    )
  }
  if (plan === undefined) {
    return <p className="text-body text-muted">Working out what moves…</p>
  }
  return (
    <PlanDetails
      replaceNewer={replaceNewer}
      plan={plan}
      destinationLabel={destinationLabel}
      onReplaceNewerChange={onReplaceNewerChange}
    />
  )
}

/**
 * The plan: its summary, the apps that quit first, its notes, the choice to
 * replace a newer copy when there is one, and the files, folded away.
 */
function PlanDetails({ replaceNewer, plan, destinationLabel, onReplaceNewerChange }: PlanDetailsProps) {
  return (
    <div className="flex flex-col gap-2">
      <p className="text-body text-ink-soft">{plan.summary}</p>
      {plan.appsToQuit.length === 0 ? null : (
        <p className="text-meta text-muted-strong">
          {plan.appsToQuit.map((app) => app.label).join(' and ')} quit first. Open them again afterwards.
        </p>
      )}
      {plan.notes.map((note) => (
        <p key={note} className="text-meta text-muted-strong">
          {note}
        </p>
      ))}
      {plan.destinationNewer ? (
        <label className="flex cursor-pointer items-start gap-2 text-body text-ink-soft">
          <input
            checked={replaceNewer}
            type="checkbox"
            onChange={(event) => onReplaceNewerChange(event.target.checked)}
            className="mt-[3px] h-4 w-4 cursor-pointer accent-orange"
          />
          <span>
            Replace newer copy
            <span className="block text-meta text-muted">
              {destinationLabel} used this session more recently. Its copy is backed up first.
            </span>
          </span>
        </label>
      ) : null}
      {plan.items.length === 0 ? null : (
        <details className="text-meta text-muted-strong">
          <summary className="cursor-pointer">Files</summary>
          <ul className="mt-1 flex flex-col gap-0.5 font-mono text-mono">
            {plan.items.map((item) => (
              <li key={item.path} className="flex justify-between gap-3">
                <span className="min-w-0 truncate">{item.path}</span>
                <span className="shrink-0">{itemActionLabels[item.action]}</span>
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  )
}

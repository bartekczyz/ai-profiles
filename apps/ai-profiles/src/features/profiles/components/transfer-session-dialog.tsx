import type { SessionSummary, TransferPlan, TransferReport, TransferRequest } from '@/lib/types'

import { useMemo, useState } from 'react'

import { Button, Dialog, Kbd } from '@/design'
import { useAppState } from '@/lib/app-state/use-app-state'
import { formatBytes } from '@/lib/format-bytes'

import { useTransferPlan, useTransferSession } from '../api/use-profile-sessions'
import { useProfiles } from '../api/use-profiles'
import { appsToQuitNote } from './apps-to-quit'
import { sessionErrorMessage } from './session-error-message'
import { shortenHomePath } from './shorten-home-path'

const stockId = 'default:claude'

type Props = {
  open: boolean
  /** Profile id the session is in, or `default:claude`. */
  sourceId: string
  session: SessionSummary
  /**
   * Move it to this profile, with nothing to choose: for bringing a session
   * that a profile's desktop app left in the Default folder into the profile.
   * Its desktop app already lists it, and the Default copy goes.
   */
  destinationId?: string
  onClose: () => void
}

/**
 * Moves one session to another Claude profile. The plan is read from the
 * backend as the options change, so what the dialog promises is what the move
 * will do, and anything that stops it (an app to quit first) shows before the
 * button is pressed. It is re-read when the window regains focus: quitting
 * that app happens outside ai-profiles.
 */
export function TransferSessionDialog({ open, sourceId, session, destinationId: fixedDestinationId, onClose }: Props) {
  const { profiles } = useProfiles()
  const defaultNames = useAppState().state.defaultProfileNames
  const destinations = useMemo(() => {
    const managed = profiles
      .filter((profile) => profile.app === 'claude' && profile.id !== sourceId)
      .map((profile) => ({ id: profile.id, label: profile.name }))
    // The stock entry by the name the user gave it, as the sidebar shows it.
    const stockLabel = defaultNames?.claude ?? 'Default (stock install)'
    return sourceId === stockId ? managed : [{ id: stockId, label: stockLabel }, ...managed]
  }, [profiles, sourceId, defaultNames])

  const [destinationId, setDestinationId] = useState(fixedDestinationId ?? destinations[0]?.id ?? '')
  const fixedLabel = fixedDestinationId
    ? profiles.find((profile) => profile.id === fixedDestinationId)?.name
    : undefined
  const [afterwards, setAfterwards] = useState<Afterwards>('archive')
  const [replaceNewer, setReplaceNewer] = useState(false)
  const [report, setReport] = useState<TransferReport | null>(null)
  const [moveError, setMoveError] = useState<string | null>(null)

  const request: TransferRequest | null = destinationId
    ? {
        sourceId,
        sessionId: session.id,
        destinationId,
        // A session moved between Claude profiles goes to the destination's
        // desktop app whenever it has one: the plan says when it can't.
        addToDesktop: true,
        archiveSource: afterwards === 'archive',
        deleteSource: afterwards === 'delete',
      }
    : null
  const plan = useTransferPlan(report ? null : request)
  const move = useTransferSession()

  const ready =
    request !== null &&
    plan.data !== undefined &&
    plan.data.blockers.length === 0 &&
    (!plan.data.destinationNewer || replaceNewer) &&
    !move.isPending

  const appsToQuit = plan.data?.appsToQuit ?? []

  async function handleMove() {
    if (!ready || !request) {
      return
    }
    setMoveError(null)
    try {
      setReport(await move.mutateAsync({ ...request, replaceNewer, quitApps: appsToQuit.length > 0 }))
    } catch (caught) {
      setMoveError(sessionErrorMessage(caught, 'The session could not be moved.'))
      await plan.refetch()
    }
  }

  const title = session.title ?? session.lastPrompt ?? session.id

  if (report) {
    return (
      <Dialog
        open={open}
        title="Session moved"
        description={title}
        onClose={onClose}
        onSubmit={onClose}
        foot={
          <Button variant="primary" size="sm" trailingKbd={<Kbd>⏎</Kbd>} onClick={onClose}>
            Done
          </Button>
        }
      >
        <ReportBody report={report} destinationLabel={plan.data?.destinationLabel} />
      </Dialog>
    )
  }

  return (
    <Dialog
      open={open}
      title={fixedDestinationId ? 'Move session here' : 'Move session'}
      description={title}
      onClose={onClose}
      onSubmit={handleMove}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={move.isPending} onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="sm" trailingKbd={<Kbd>⏎</Kbd>} disabled={!ready} onClick={handleMove}>
            {move.isPending
              ? appsToQuit.length > 0
                ? 'Quitting and moving…'
                : 'Moving…'
              : appsToQuit.length > 0
                ? 'Quit and move'
                : 'Move'}
          </Button>
        </>
      }
    >
      {fixedDestinationId ? (
        <div className="space-y-3">
          {session.cwd ? <p className="font-mono text-mono text-muted-strong">{shortenHomePath(session.cwd)}</p> : null}
          <p className="text-body text-ink-soft">
            {fixedLabel ?? 'This profile'}'s desktop app lists this session, but its conversation was saved in the
            Default folder, before the profile had one of its own. Moving it brings the conversation into{' '}
            {fixedLabel ?? 'the profile'}. The Default copy is kept in session-transfer-backups.
          </p>
          <PlanBody
            plan={plan.data}
            loading={plan.isLoading}
            error={plan.error ? sessionErrorMessage(plan.error) : null}
            replaceNewer={replaceNewer}
            onReplaceNewer={setReplaceNewer}
            onRecheck={() => void plan.refetch()}
          />
          {moveError ? (
            <p role="alert" className="text-meta text-red">
              {moveError}
            </p>
          ) : null}
        </div>
      ) : destinations.length === 0 ? (
        <p className="text-body text-ink-soft">There is no other Claude profile to move it to.</p>
      ) : (
        <div className="space-y-3">
          {session.cwd ? <p className="font-mono text-mono text-muted-strong">{shortenHomePath(session.cwd)}</p> : null}

          <label className="flex items-center gap-2 text-body text-ink-soft">
            <span className="w-8 shrink-0">To</span>
            <select
              className="h-8 flex-1 cursor-pointer rounded-[7px] border border-border bg-white/60 px-2 text-body text-ink dark:bg-white/[0.05]"
              value={destinationId}
              onChange={(event) => {
                setDestinationId(event.target.value)
                setReplaceNewer(false)
              }}
            >
              {destinations.map((destination) => (
                <option key={destination.id} value={destination.id}>
                  {destination.label}
                </option>
              ))}
            </select>
          </label>

          {plan.data ? <DesktopLine plan={plan.data} /> : null}
          <AfterwardsChoice
            source={plan.data?.sourceLabel ?? 'this profile'}
            sourceBytes={plan.data?.sourceBytes ?? null}
            value={afterwards}
            onChange={setAfterwards}
          />

          <PlanBody
            plan={plan.data}
            loading={plan.isLoading}
            error={plan.error ? sessionErrorMessage(plan.error) : null}
            replaceNewer={replaceNewer}
            onReplaceNewer={setReplaceNewer}
            onRecheck={() => void plan.refetch()}
          />

          {moveError ? (
            <p role="alert" className="text-meta text-red">
              {moveError}
            </p>
          ) : null}
        </div>
      )}
    </Dialog>
  )
}

function PlanBody({
  plan,
  loading,
  error,
  replaceNewer,
  onReplaceNewer,
  onRecheck,
}: {
  plan: TransferPlan | undefined
  loading: boolean
  error: string | null
  replaceNewer: boolean
  onReplaceNewer: (value: boolean) => void
  onRecheck: () => void
}) {
  if (loading) {
    return <p className="text-meta text-muted">Checking…</p>
  }
  if (error) {
    return (
      <p role="alert" className="text-meta text-red">
        {error}
      </p>
    )
  }
  if (!plan) {
    return null
  }
  const count = (action: string) => plan.items.filter((item) => item.action === action).length
  const [copy, replace, same] = [count('copy'), count('replace'), count('same')]
  const summary = [
    copy ? `${copy} to copy` : null,
    replace ? `${replace} to replace (backed up first)` : null,
    same ? `${same} already there` : null,
  ]
    .filter(Boolean)
    .join(', ')

  return (
    <div className="space-y-2 rounded-[10px] border border-border-soft bg-white/30 px-[13px] py-[9px] dark:bg-white/[0.02]">
      <p className="text-meta text-ink-soft">Files: {summary}.</p>
      {plan.destinationNewer ? (
        <label className="flex cursor-pointer items-start gap-2 text-meta text-amber">
          <input
            type="checkbox"
            checked={replaceNewer}
            onChange={(event) => onReplaceNewer(event.target.checked)}
            className="mt-[2px] h-3.5 w-3.5 cursor-pointer accent-orange"
          />
          {plan.destinationLabel} has a newer copy of this session. Replace it anyway?
        </label>
      ) : null}
      {plan.blockers.length > 0 ? (
        <div role="alert" className="space-y-1">
          {plan.blockers.map((blocker) => (
            <p key={blocker} className="text-meta text-red">
              {blocker}
            </p>
          ))}
          <button type="button" className="cursor-pointer text-meta text-muted-strong underline" onClick={onRecheck}>
            Check again
          </button>
        </div>
      ) : null}
      {plan.appsToQuit.length > 0 && plan.blockers.length === 0 ? (
        <p className="text-meta text-amber">{appsToQuitNote(plan.appsToQuit)}</p>
      ) : null}
      {plan.notes.map((note) => (
        <p key={note} className="text-meta text-muted">
          {note}
        </p>
      ))}
    </div>
  )
}

/** What becomes of the copy the session leaves behind. */
type Afterwards = 'archive' | 'delete' | 'keep'

/** Whether the destination's desktop app will list the session. */
function DesktopLine({ plan }: { plan: TransferPlan }) {
  const text =
    plan.desktop === 'add'
      ? `${plan.destinationLabel}'s desktop app will list it too.`
      : plan.desktop === 'alreadyListed'
        ? `${plan.destinationLabel}'s desktop app already lists it.`
        : plan.desktop === 'unavailable'
          ? `Not in ${plan.destinationLabel}'s desktop app: ${plan.desktopReason ?? "it can't be added."}`
          : null
  return text ? <p className="text-meta text-muted">{text}</p> : null
}

/**
 * What to do with the copy left in the profile the session moves from.
 * Archiving keeps its transcript, which can be large, in the backups;
 * deleting frees that, once the moved copy is checked, and can't be undone.
 */
function AfterwardsChoice({
  source,
  sourceBytes,
  value,
  onChange,
}: {
  source: string
  /** What deleting frees, once the plan says. */
  sourceBytes: number | null
  value: Afterwards
  onChange: (value: Afterwards) => void
}) {
  const size = sourceBytes === null ? null : formatBytes(sourceBytes)
  const options: Array<{ value: Afterwards; label: string; hint: string }> = [
    {
      value: 'archive',
      label: size === null ? 'Archive it' : `Archive it (${size})`,
      hint: 'Only one profile lists it. Its transcript is kept in session-transfer-backups, and can be restored.',
    },
    {
      value: 'delete',
      label: 'Delete it',
      hint: "Only once the moved copy is checked to be identical. Can't be undone. Plan files and project memory stay.",
    },
    { value: 'keep', label: 'Keep it', hint: 'Both profiles list it.' },
  ]
  return (
    <fieldset className="space-y-1.5">
      <legend className="mb-1.5 text-body text-ink-soft">The copy in {source}, once it has moved</legend>
      {options.map((option) => (
        <label key={option.value} className="flex cursor-pointer items-start gap-2 text-body text-ink-soft">
          <input
            type="radio"
            name="afterwards"
            value={option.value}
            checked={value === option.value}
            onChange={() => onChange(option.value)}
            className="mt-[3px] h-4 w-4 cursor-pointer accent-orange"
          />
          <span>
            {option.label}
            <span className="block text-meta text-muted">{option.hint}</span>
          </span>
        </label>
      ))}
      {value === 'keep' ? (
        <p
          role="note"
          className="rounded-md border border-amber/40 bg-amber/[0.08] px-2.5 py-2 text-meta text-ink-soft"
        >
          This forks the session: two copies that go their own ways from here. Moving it back later doesn't merge them.
          If the copy it lands on has changed since, the move asks before replacing it, and backs it up in
          session-transfer-backups first.
        </p>
      ) : null}
    </fieldset>
  )
}

function ReportBody({ report, destinationLabel }: { report: TransferReport; destinationLabel?: string }) {
  return (
    <div className="space-y-2 text-body text-ink-soft">
      <p>
        It's now in {destinationLabel ?? 'the other profile'}
        {report.desktopRecord ? ', and its desktop app lists it.' : '.'}
      </p>
      {report.archivedTo ? (
        <p className="text-meta text-muted">
          The original went to <code className="font-mono text-mono">{shortenHomePath(report.archivedTo)}</code>.
        </p>
      ) : null}
      {report.freedBytes !== null ? <p className="text-meta text-muted">The original was deleted.</p> : null}
      {report.deleteError ? (
        <p role="alert" className="text-meta text-amber">
          {report.deleteError}
        </p>
      ) : null}
      {report.backupDir ? (
        <p className="text-meta text-muted">
          What it replaced went to <code className="font-mono text-mono">{shortenHomePath(report.backupDir)}</code>.
        </p>
      ) : null}
      {report.memoryConflicts.length > 0 ? (
        <p className="text-meta text-amber">
          Project memory that differed was left as the destination had it: {report.memoryConflicts.join(', ')}.
        </p>
      ) : null}
    </div>
  )
}

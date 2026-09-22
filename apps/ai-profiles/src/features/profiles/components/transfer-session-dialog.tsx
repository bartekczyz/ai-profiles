import type { SessionSummary, TransferPlan, TransferReport, TransferRequest } from '@/lib/types'

import { useMemo, useState } from 'react'

import { Button, Dialog, Kbd } from '@/design'

import { useTransferPlan, useTransferSession } from '../api/use-profile-sessions'
import { useProfiles } from '../api/use-profiles'
import { appsToQuitLabel } from './apps-to-quit'
import { sessionErrorMessage } from './session-error-message'
import { shortenHomePath } from './shorten-home-path'

const stockId = 'default:claude'

type Props = {
  open: boolean
  /** Profile id the session is in, or `default:claude`. */
  sourceId: string
  session: SessionSummary
  onClose: () => void
}

/**
 * Moves one session to another Claude profile. The plan is read from the
 * backend as the options change, so what the dialog promises is what the move
 * will do, and anything that stops it (an app to quit first) shows before the
 * button is pressed. It is re-read when the window regains focus: quitting
 * that app happens outside ai-profiles.
 */
export function TransferSessionDialog({ open, sourceId, session, onClose }: Props) {
  const { profiles } = useProfiles()
  const destinations = useMemo(() => {
    const managed = profiles
      .filter((profile) => profile.app === 'claude' && profile.id !== sourceId)
      .map((profile) => ({ id: profile.id, label: profile.name }))
    return sourceId === stockId ? managed : [{ id: stockId, label: 'Default (stock install)' }, ...managed]
  }, [profiles, sourceId])

  const [destinationId, setDestinationId] = useState(destinations[0]?.id ?? '')
  const [addToDesktop, setAddToDesktop] = useState(true)
  const [archiveSource, setArchiveSource] = useState(true)
  const [replaceNewer, setReplaceNewer] = useState(false)
  const [report, setReport] = useState<TransferReport | null>(null)
  const [moveError, setMoveError] = useState<string | null>(null)

  const request: TransferRequest | null = destinationId
    ? { sourceId, sessionId: session.id, destinationId, addToDesktop, archiveSource }
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
      title="Move session"
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
                ? `Quit ${appsToQuitLabel(appsToQuit)} and move`
                : 'Move'}
          </Button>
        </>
      }
    >
      {destinations.length === 0 ? (
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

          <Checkbox
            checked={addToDesktop}
            onChange={setAddToDesktop}
            label="Add it to that profile's desktop app"
            hint={
              plan.data?.desktop === 'unavailable'
                ? (plan.data.desktopReason ?? undefined)
                : plan.data?.desktop === 'alreadyListed'
                  ? 'The desktop app already lists it.'
                  : undefined
            }
          />
          <Checkbox
            checked={archiveSource}
            onChange={setArchiveSource}
            label="Take it out of this profile afterwards"
            hint="Its transcript is kept in session-transfer-backups, so it can be put back."
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

function Checkbox({
  checked,
  onChange,
  label,
  hint,
}: {
  checked: boolean
  onChange: (value: boolean) => void
  label: string
  hint?: string
}) {
  return (
    <label className="flex cursor-pointer items-start gap-2 text-body text-ink-soft">
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
        className="mt-[3px] h-4 w-4 cursor-pointer accent-orange"
      />
      <span>
        {label}
        {hint ? <span className="block text-meta text-muted">{hint}</span> : null}
      </span>
    </label>
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
        <p className="text-meta text-amber">
          {appsToQuitLabel(plan.appsToQuit)} will quit first: it has the session open or keeps the list it's changing.
          Its other sessions close too, and come back when you open it again.
        </p>
      ) : null}
      {plan.notes.map((note) => (
        <p key={note} className="text-meta text-muted">
          {note}
        </p>
      ))}
    </div>
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

import type { AppDockIconSpec } from '@/lib/app-registry'
import type { AppId } from '@/lib/types'

import { useState } from 'react'

import { Button, cn, Dialog, Kbd } from '@/design'
import { appSpecs } from '@/lib/app-registry'

type Props = {
  open: boolean
  app: AppId
  onClose: () => void
  /**
   * Called when the user confirms. It deals with its own failures, and closes
   * the dialog when it succeeds. Without it the dialog asks nothing: it only
   * explains, and has a single button to close it.
   */
  onConfirm?: () => Promise<void>
}

/**
 * One point of comparison: what the profile is like with and without a Dock icon
 * of its own.
 */
type Comparison = {
  /**
   * What it is like without one. `null` when there is nothing to say.
   */
  off: string | null
  /**
   * What it is like with one.
   */
  on: string
}

/**
 * What changes with a Dock icon of its own. Everything here is a consequence of
 * how it is done ({@link AppDockIconSpec.shape}): a wrapper is a re-signed copy
 * of the app, which macOS sees as an app in its own right; a signed copy is the
 * developer's app as it is, with only its icon set on this Mac.
 */
function comparisons(displayName: string, dockIcon: AppDockIconSpec): Array<Comparison> {
  const cost = dockIcon.cost !== null ? [{ off: null, on: dockIcon.cost }] : []
  const dock = {
    off: `In the Dock and ⌘-Tab, every profile looks like ${displayName}.`,
    on: 'In the Dock and ⌘-Tab, this profile has its own icon, badged with its color, and its own name.',
  }
  if (dockIcon.shape === 'signedCopy') {
    return [
      dock,
      {
        off: `The original ${displayName}, signed by its developer.`,
        on: `A copy of ${displayName}, still signed by its developer. Only its icon is set on this Mac, so sign-in, permissions and passkeys work as they normally do.`,
      },
      ...cost,
      {
        off: `${displayName} updates itself, as usual.`,
        on: `The copy updates itself, like ${displayName} does. An update resets its icon, which comes back the next time you open the profile.`,
      },
    ]
  }
  return [
    dock,
    {
      off: `The original ${displayName}, signed by its developer.`,
      on: "A copy, signed on this Mac instead. macOS doesn't run its Gatekeeper checks on it, and it never leaves this Mac.",
    },
    {
      off: 'Sign-in, permissions and passkeys work as they normally do.',
      on: 'macOS sees a different app, so the profile has to sign in again, permissions like the microphone and camera are asked for again, and passkeys may not work.',
    },
    ...cost,
    {
      off: `${displayName} updates itself, as usual.`,
      on: `After ${displayName} updates, opening this profile first makes a fresh copy of the new version. That takes several seconds.`,
    },
  ]
}

const columnHeadClass = 'pb-2 text-left font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong'

function Point({ children }: { children: string | null }) {
  return (
    <td className="border-t border-border-soft py-2.5 align-top first:pr-5 last:border-l last:pl-5">
      {children === null ? null : (
        <p className="relative pl-3.5 text-body text-muted before:absolute before:left-0 before:content-['•']">
          {children}
        </p>
      )}
    </td>
  )
}

function ComparisonTable({ rows }: { rows: Array<Comparison> }) {
  return (
    <table className="w-full table-fixed border-collapse">
      <caption className="sr-only">What changes with a Dock icon of its own</caption>
      <thead>
        <tr>
          <th scope="col" className={cn(columnHeadClass, 'pr-5')}>
            Off
          </th>
          <th scope="col" className={cn(columnHeadClass, 'pl-5')}>
            On
          </th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr key={row.on}>
            <Point>{row.off}</Point>
            <Point>{row.on}</Point>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

/**
 * What a Dock icon of its own involves: the two ways a profile can be, side by
 * side. It is shown as a question the first time the user turns the setting on,
 * and can be opened to read at any time. It opens over the form the setting is
 * in.
 */
export function DockIconConsentDialog({ open, app, onClose, onConfirm }: Props) {
  const [submitting, setSubmitting] = useState(false)
  const { displayName, dockIcon } = appSpecs[app]

  async function handleConfirm() {
    if (submitting || onConfirm === undefined) {
      return
    }
    setSubmitting(true)
    try {
      await onConfirm()
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog
      stacked
      open={open}
      title={onConfirm === undefined ? 'A Dock icon of its own' : 'Give this profile its own Dock icon?'}
      description={`ai-profiles makes a copy of ${displayName} for this profile, using almost no extra disk space${dockIcon.shape === 'signedCopy' ? ' until the copy updates' : ''}. You can turn it off again in Edit profile, which deletes the copy and goes back to the regular app. Your data stays where it is either way.`}
      className="w-[min(780px,calc(100%-64px))]"
      onClose={onClose}
      onSubmit={onConfirm === undefined ? onClose : handleConfirm}
      foot={
        onConfirm === undefined ? (
          <Button variant="primary" size="sm" trailingKbd={<Kbd variant="onOrange">⏎</Kbd>} onClick={onClose}>
            Close
          </Button>
        ) : (
          <>
            <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={submitting} onClick={onClose}>
              Cancel
            </Button>
            <Button
              variant="primary"
              size="sm"
              trailingKbd={<Kbd variant="onOrange">⏎</Kbd>}
              disabled={submitting}
              onClick={handleConfirm}
            >
              Turn on
            </Button>
          </>
        )
      }
    >
      <ComparisonTable rows={comparisons(displayName, dockIcon)} />
    </Dialog>
  )
}

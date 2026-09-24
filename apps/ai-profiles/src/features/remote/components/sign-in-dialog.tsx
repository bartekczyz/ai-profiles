import type { AppError, LoginStart, RemoteHost } from '@/lib/types'

import { useEffect, useRef, useState } from 'react'

import { ExternalLink, RotateCw } from 'lucide-react'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { Input } from '@/design/ui/input'
import { sessionErrorMessage } from '@/features/profiles/components/session-error-message'
import { openExternalUrl } from '@/lib/commands'

import { useRemoteSignIn } from '../api/use-remote'

type Props = {
  open: boolean
  host: RemoteHost
  account: string
  /** What Cancel says: "Skip for now" straight after making the profile. */
  cancelLabel?: string
  onClose: () => void
}

/** The host's sign-in is over after these: only starting again helps. */
const endedCodes = new Set(['login_failed', 'login_expired'])

function errorCode(error: unknown): string | undefined {
  return error && typeof error === 'object' && 'code' in error ? (error as AppError).code : undefined
}

/**
 * Signing a remote account in from the Mac. The host runs `claude auth
 * login`, its sign-in page opens here in the browser, and the code the page
 * shows is pasted back and typed in on the host.
 */
export function SignInDialog({ open, host, account, cancelLabel = 'Cancel', onClose }: Props) {
  const { start, submit, cancel } = useRemoteSignIn(host.id, account)
  const toast = useToast()
  const [login, setLogin] = useState<LoginStart | null>(null)
  const [code, setCode] = useState('')
  const openRef = useRef(open)
  openRef.current = open

  async function begin() {
    setLogin(null)
    setCode('')
    submit.reset()
    try {
      const started = await start.mutateAsync()
      if (openRef.current) {
        setLogin(started)
      } else {
        // Closed while the host was starting it.
        void cancel(started.loginId)
      }
    } catch {
      // Shown below.
    }
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: a fresh sign-in each time the dialog opens
  useEffect(() => {
    if (open) {
      start.reset()
      void begin()
    }
  }, [open])

  const ended = submit.isError && endedCodes.has(errorCode(submit.error) ?? '')
  const ready = login !== null && code.trim().length > 0 && !submit.isPending && !ended

  async function handleSubmit() {
    if (!ready || login === null) {
      return
    }
    try {
      const signed = await submit.mutateAsync({ loginId: login.loginId, code: code.trim() })
      const who = signed.account?.email
      toast.success(`Signed in ${account}`, who ? `As ${who}, on ${host.label}.` : `On ${host.label}.`)
      setLogin(null)
      onClose()
    } catch {
      // Shown below.
    }
  }

  function handleClose() {
    if (login !== null && !ended) {
      void cancel(login.loginId)
    }
    setLogin(null)
    onClose()
  }

  return (
    <Dialog
      open={open}
      title={`Sign in ${account}`}
      description={`On ${host.label}. Sign in in the browser, then paste the code the page shows.`}
      onClose={handleClose}
      onSubmit={handleSubmit}
      closeOnOutsideClick={false}
      foot={
        <>
          <Button
            variant="ghost"
            size="sm"
            trailingKbd={<Kbd>⎋</Kbd>}
            disabled={submit.isPending}
            onClick={handleClose}
          >
            {cancelLabel}
          </Button>
          <Button
            variant="primary"
            size="sm"
            trailingKbd={<Kbd variant="onOrange">⏎</Kbd>}
            disabled={!ready}
            onClick={handleSubmit}
          >
            {submit.isPending ? 'Signing in…' : 'Sign in'}
          </Button>
        </>
      }
    >
      <div className="space-y-3 text-body text-ink-soft">
        {start.isPending ? (
          <p className="text-muted">Asking {host.label} for a sign-in page…</p>
        ) : start.isError ? (
          <Problem
            message={sessionErrorMessage(start.error, 'Signing in could not start.')}
            onRetry={() => void begin()}
          />
        ) : login ? (
          <>
            <p>
              The sign-in page is open in your browser. Sign in with the account {account} should use, then copy the
              code it shows.
            </p>
            <Button
              variant="secondary"
              size="sm"
              leadingIcon={<ExternalLink className="h-3.5 w-3.5" />}
              onClick={() => void openExternalUrl(login.url)}
            >
              Open the page again
            </Button>
            <div>
              <label
                htmlFor="remote-sign-in-code"
                className="mb-1.5 block font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted"
              >
                Code
              </label>
              <Input
                id="remote-sign-in-code"
                value={code}
                autoFocus
                maxLength={512}
                disabled={ended}
                onChange={(event) => setCode(event.target.value)}
                placeholder="Paste the code here"
                autoComplete="off"
                spellCheck={false}
              />
            </div>
          </>
        ) : null}
        {submit.isError ? (
          ended ? (
            <Problem
              message={sessionErrorMessage(submit.error)}
              retryLabel="Sign in again"
              onRetry={() => void begin()}
            />
          ) : (
            <p role="alert" className="text-meta text-red">
              {sessionErrorMessage(submit.error, 'Signing in did not work.')}
            </p>
          )
        ) : null}
      </div>
    </Dialog>
  )
}

function Problem({
  message,
  retryLabel = 'Try again',
  onRetry,
}: {
  message: string
  retryLabel?: string
  onRetry: () => void
}) {
  return (
    <div className="flex items-start justify-between gap-3">
      <p role="alert" className="text-meta text-red">
        {message}
      </p>
      <Button variant="ghost" size="sm" leadingIcon={<RotateCw className="h-3.5 w-3.5" />} onClick={onRetry}>
        {retryLabel}
      </Button>
    </div>
  )
}

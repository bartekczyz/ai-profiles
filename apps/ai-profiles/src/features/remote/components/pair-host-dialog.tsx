import type { PairingPreview } from '@/lib/types'

import { useEffect, useState } from 'react'

import { Button, Dialog, Kbd } from '@/design'
import { Input } from '@/design/ui/input'
import { sessionErrorMessage } from '@/features/profiles/components/session-error-message'
import { remotePreviewPairing } from '@/lib/commands'

import { usePairHost } from '../api/use-remote'

type Props = {
  open: boolean
  onClose: () => void
  onPaired?: (label: string) => void
}

const labelClasses = 'mb-1.5 block font-mono text-[11.5px] font-medium uppercase tracking-[0.08em] text-muted'

/**
 * Pairing with a remote host: paste the code `ai-profiles-server pair`
 * printed. The code is read here first, so the addresses and the
 * certificate's fingerprint can be checked against what the server printed
 * before anything is sent.
 */
export function PairHostDialog({ open, onClose, onPaired }: Props) {
  const [code, setCode] = useState('')
  const [label, setLabel] = useState('')
  const [preview, setPreview] = useState<PairingPreview | null>(null)
  const [codeError, setCodeError] = useState<string | null>(null)
  const pairing = usePairHost()

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset each time the dialog opens
  useEffect(() => {
    if (open) {
      setCode('')
      setLabel('')
      setPreview(null)
      setCodeError(null)
      pairing.reset()
    }
  }, [open])

  useEffect(() => {
    const trimmed = code.trim()
    if (trimmed.length === 0) {
      setPreview(null)
      setCodeError(null)
      return
    }
    let cancelled = false
    remotePreviewPairing(trimmed)
      .then((read) => {
        if (!cancelled) {
          setPreview(read)
          setCodeError(null)
        }
      })
      .catch((caught) => {
        if (!cancelled) {
          setPreview(null)
          setCodeError(sessionErrorMessage(caught, "That isn't a pairing code."))
        }
      })
    return () => {
      cancelled = true
    }
  }, [code])

  const ready = preview !== null && !pairing.isPending

  async function handlePair() {
    if (!ready) {
      return
    }
    try {
      const host = await pairing.mutateAsync({ code: code.trim(), label: label.trim() || undefined })
      onPaired?.(host.label)
      onClose()
    } catch {
      // Shown below from the mutation's error.
    }
  }

  return (
    <Dialog
      open={open}
      title="Pair a remote host"
      description="On the Linux machine, run ai-profiles-server pair and paste the code it prints."
      onClose={onClose}
      onSubmit={handlePair}
      closeOnOutsideClick={false}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={pairing.isPending} onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="sm"
            trailingKbd={<Kbd variant="onOrange">⏎</Kbd>}
            disabled={!ready}
            onClick={handlePair}
          >
            {pairing.isPending ? 'Pairing…' : 'Pair'}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div>
          <label htmlFor="pairing-code" className={labelClasses}>
            Pairing code
          </label>
          <textarea
            id="pairing-code"
            autoFocus
            rows={3}
            value={code}
            onChange={(event) => setCode(event.target.value)}
            placeholder="aip1.…"
            spellCheck={false}
            autoCorrect="off"
            autoCapitalize="off"
            className="w-full resize-none rounded-md border border-border bg-white px-2.5 py-2 font-mono text-[12px] text-ink outline-none focus:border-orange dark:bg-cream-2"
          />
          {codeError ? <p className="mt-1 text-meta text-red">{codeError}</p> : null}
        </div>

        {preview ? (
          <div className="space-y-1.5 rounded-[10px] border border-border-soft bg-white/30 px-[13px] py-[9px] text-meta dark:bg-white/[0.02]">
            <p className="text-ink-soft">
              Reaches it at <span className="font-mono text-mono">{preview.addresses.join(', ')}</span>
            </p>
            <p className="text-ink-soft">Certificate fingerprint:</p>
            <p className="break-all font-mono text-mono text-muted-strong">{preview.fingerprint}</p>
            <p className="text-muted">
              It should match the one the server printed. ai-profiles will trust this certificate and no other for this
              host.
            </p>
          </div>
        ) : null}

        <div>
          <label htmlFor="host-label" className={labelClasses}>
            Name <span className="normal-case tracking-normal text-muted">(optional)</span>
          </label>
          <Input
            id="host-label"
            value={label}
            maxLength={64}
            onChange={(event) => setLabel(event.target.value)}
            placeholder="Its host name"
            autoComplete="off"
            spellCheck={false}
          />
        </div>

        {pairing.isError ? (
          <p role="alert" className="text-meta text-red">
            {sessionErrorMessage(pairing.error, 'Pairing failed.')}
          </p>
        ) : null}
      </div>
    </Dialog>
  )
}

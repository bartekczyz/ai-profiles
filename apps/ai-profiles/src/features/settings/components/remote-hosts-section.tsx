import type { ReactNode } from 'react'
import type { RemoteHost } from '@/lib/types'

import { useEffect, useState } from 'react'

import { Check, ChevronRight, Copy, Plus } from 'lucide-react'

import { Button, cn, Dialog, Kbd, Skeleton, StatusDot, useToast } from '@/design'
import { Input } from '@/design/ui/input'
import { sessionErrorMessage } from '@/features/profiles/components/session-error-message'
import { useRemoteHostInfo, useRemoteHosts, useRemoveHost, useRenameHost } from '@/features/remote/api/use-remote'
import { PairHostDialog } from '@/features/remote/components/pair-host-dialog'
import { copyToClipboard } from '@/lib/commands'

const headingClasses = 'font-mono text-[10px] font-medium uppercase tracking-[0.1em] text-muted-strong'

/**
 * Linux machines running ai-profiles-server. Pairing one adds a section for
 * it to the sidebar with its Claude accounts.
 */
export function RemoteHostsSection() {
  const hosts = useRemoteHosts()
  const toast = useToast()
  const [pairing, setPairing] = useState(false)
  const [renaming, setRenaming] = useState<RemoteHost | null>(null)
  const [removing, setRemoving] = useState<RemoteHost | null>(null)

  return (
    <section className="mb-8">
      <div className="mb-2.5 flex items-center justify-between">
        <span className={headingClasses}>Remote hosts</span>
        <Button
          size="sm"
          variant="secondary"
          leadingIcon={<Plus className="h-3.5 w-3.5" strokeWidth={1.85} />}
          onClick={() => setPairing(true)}
        >
          Pair a host
        </Button>
      </div>
      <div className="rounded-xl border border-border bg-white py-1 dark:bg-cream-2">
        {hosts.length === 0 ? (
          <p className="px-4 py-3 text-[13px] text-muted">
            None yet. Install ai-profiles-server on a Linux machine where Claude Code runs, then pair it here to see its
            accounts and sessions.
          </p>
        ) : (
          hosts.map((host) => (
            <HostRow key={host.id} host={host} onRename={() => setRenaming(host)} onRemove={() => setRemoving(host)} />
          ))
        )}
      </div>
      <SetupGuide initiallyOpen={hosts.length === 0} onPair={() => setPairing(true)} />

      <PairHostDialog
        open={pairing}
        onClose={() => setPairing(false)}
        onPaired={(label) => toast.success(`Paired with ${label}.`)}
      />
      {renaming ? <RenameHostDialog host={renaming} onClose={() => setRenaming(null)} /> : null}
      {removing ? <RemoveHostDialog host={removing} onClose={() => setRemoving(null)} /> : null}
    </section>
  )
}

/**
 * How to get a Linux machine ready to pair, step by step: what it needs,
 * installing the server, and its `setup`, which walks through the rest and
 * ends with the code to paste here. Open while no host is paired.
 */
function SetupGuide({ initiallyOpen, onPair }: { initiallyOpen: boolean; onPair: () => void }) {
  const [open, setOpen] = useState(initiallyOpen)
  return (
    <div className="mt-2.5">
      <button
        type="button"
        aria-expanded={open}
        className="flex cursor-pointer items-center gap-1 text-[12.5px] text-muted-strong hover:text-ink-soft"
        onClick={() => setOpen((value) => !value)}
      >
        <ChevronRight aria-hidden className={cn('h-3.5 w-3.5 transition-transform', open && 'rotate-90')} />
        How to set up a host
      </button>
      {open ? (
        <ol className="mt-2 space-y-3 rounded-xl border border-border-soft px-4 py-3 text-[12.5px] leading-[1.5] text-ink-soft">
          <GuideStep number={1} title="On the Linux machine, install tmux 3.0 or newer, and Claude Code">
            <p className="text-muted">
              Every session runs in a tmux window, so it keeps going when nothing is attached. Sign-ins can happen
              later, from here.
            </p>
            <CopyLine text="sudo apt install tmux" />
            <CopyLine text="curl -fsSL https://claude.ai/install.sh | bash" />
          </GuideStep>
          <GuideStep number={2} title="Install ai-profiles-server">
            <p className="text-muted">A prebuilt binary, for x86_64 or arm64 Linux:</p>
            <CopyLine text="mkdir -p ~/.local/bin && curl -fsSL https://github.com/bartekczyz/ai-profiles/releases/latest/download/ai-profiles-server-$(uname -m)-linux -o ~/.local/bin/ai-profiles-server && chmod +x ~/.local/bin/ai-profiles-server" />
            <p className="text-muted">Or built from source, which takes a Rust toolchain (rustup.rs):</p>
            <CopyLine text="cargo install --locked --git https://github.com/bartekczyz/ai-profiles ai-profiles-server" />
          </GuideStep>
          <GuideStep number={3} title="Run its setup">
            <p className="text-muted">
              It checks tmux and Claude Code, finds your Claude profiles, asks which networks may connect (Tailscale and
              WireGuard peers can by default; a LAN only if you say so), installs it as a service that survives a
              reboot, and prints a pairing code.
            </p>
            <CopyLine text="ai-profiles-server setup" />
          </GuideStep>
          <GuideStep number={4} title="Pair it here">
            <p className="text-muted">
              Paste the code, and check the certificate fingerprint matches the one setup printed. A new code any time:{' '}
              <code className="font-mono text-[11.5px]">ai-profiles-server pair</code>. Something wrong:{' '}
              <code className="font-mono text-[11.5px]">ai-profiles-server doctor</code>.
            </p>
            <div>
              <Button
                size="sm"
                variant="secondary"
                leadingIcon={<Plus className="h-3.5 w-3.5" strokeWidth={1.85} />}
                onClick={onPair}
              >
                Pair a host
              </Button>
            </div>
          </GuideStep>
        </ol>
      ) : null}
    </div>
  )
}

function GuideStep({ number, title, children }: { number: number; title: string; children: ReactNode }) {
  return (
    <li className="grid grid-cols-[18px_1fr] gap-2">
      <span className="font-mono text-[11px] leading-[19px] text-muted-strong">{number}.</span>
      <div className="min-w-0 space-y-1.5">
        <p className="text-ink">{title}</p>
        {children}
      </div>
    </li>
  )
}

/** A command, with a button that copies it. */
function CopyLine({ text }: { text: string }) {
  const [copied, setCopied] = useState(false)
  return (
    <div className="flex items-center gap-2 rounded-md border border-border-soft px-2 py-1">
      <code className="min-w-0 flex-1 truncate font-mono text-[11.5px] text-muted-strong" title={text}>
        {text}
      </code>
      <button
        type="button"
        aria-label={copied ? 'Copied' : `Copy ${text}`}
        className="cursor-pointer text-muted hover:text-ink-soft"
        onClick={async () => {
          await copyToClipboard(text)
          setCopied(true)
        }}
      >
        {copied ? <Check aria-hidden className="h-3.5 w-3.5" /> : <Copy aria-hidden className="h-3.5 w-3.5" />}
      </button>
    </div>
  )
}

function HostRow({ host, onRename, onRemove }: { host: RemoteHost; onRename: () => void; onRemove: () => void }) {
  const info = useRemoteHostInfo(host.id)
  const tone = info.isError ? 'danger' : info.isSuccess ? 'success' : 'neutral'
  const status = info.isError
    ? sessionErrorMessage(info.error, 'Not answering.')
    : info.data
      ? [
          info.data.claude?.version ? `Claude ${info.data.claude.version.split(' ')[0]}` : 'no claude',
          info.data.tmux?.version ?? 'no tmux',
        ].join(' · ')
      : 'Checking…'
  return (
    <div className="grid grid-cols-[7px_1fr_auto] items-center gap-3 border-b border-border-soft px-4 py-3 text-[13px] tracking-[-0.003em] text-ink-soft last:border-b-0">
      <StatusDot tone={tone} pulse={info.isSuccess} />
      <div className="min-w-0">
        <div>
          {host.label}
          <span className="ml-2 font-mono text-[11.5px] text-muted">{host.lastGoodAddress ?? host.addresses[0]}</span>
        </div>
        <div className="truncate font-mono text-[11px] text-muted" title={status}>
          {status}
        </div>
      </div>
      <div className="flex items-center gap-1">
        <Button size="sm" variant="ghost" onClick={onRename}>
          Rename
        </Button>
        <Button size="sm" variant="ghost" onClick={onRemove}>
          Remove
        </Button>
      </div>
    </div>
  )
}

function RenameHostDialog({ host, onClose }: { host: RemoteHost; onClose: () => void }) {
  const [label, setLabel] = useState(host.label)
  const rename = useRenameHost()
  useEffect(() => setLabel(host.label), [host.label])
  const trimmed = label.trim()
  const ready = trimmed.length > 0 && trimmed !== host.label && !rename.isPending
  async function handleSave() {
    if (!ready) {
      return
    }
    try {
      await rename.mutateAsync({ hostId: host.id, label: trimmed })
      onClose()
    } catch {
      // Shown below.
    }
  }
  return (
    <Dialog
      open
      title="Rename host"
      description={`What the sidebar calls ${host.hostname}. The server isn't changed.`}
      onClose={onClose}
      onSubmit={handleSave}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} onClick={onClose}>
            Cancel
          </Button>
          <Button
            variant="primary"
            size="sm"
            trailingKbd={<Kbd variant="onOrange">⏎</Kbd>}
            disabled={!ready}
            onClick={handleSave}
          >
            Save
          </Button>
        </>
      }
    >
      <Input autoFocus value={label} maxLength={64} onChange={(event) => setLabel(event.target.value)} />
      {rename.isError ? (
        <p role="alert" className="mt-2 text-meta text-red">
          {sessionErrorMessage(rename.error)}
        </p>
      ) : null}
    </Dialog>
  )
}

function RemoveHostDialog({ host, onClose }: { host: RemoteHost; onClose: () => void }) {
  const remove = useRemoveHost()
  async function handleRemove() {
    try {
      await remove.mutateAsync(host.id)
      onClose()
    } catch {
      // Shown below.
    }
  }
  return (
    <Dialog
      open
      title={`Remove ${host.label}?`}
      description="ai-profiles forgets this host and deletes its token from the Keychain. The server is told to forget this Mac too, if it can be reached. Nothing on the host changes."
      onClose={onClose}
      onSubmit={handleRemove}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={remove.isPending} onClick={onClose}>
            Cancel
          </Button>
          <Button variant="danger" size="sm" disabled={remove.isPending} onClick={handleRemove}>
            {remove.isPending ? 'Removing…' : 'Remove'}
          </Button>
        </>
      }
    >
      {remove.isError ? (
        <p role="alert" className="text-meta text-red">
          {sessionErrorMessage(remove.error)}
        </p>
      ) : (
        <p className="text-body text-ink-soft">To see it again later, pair it with a new code.</p>
      )}
    </Dialog>
  )
}

export function RemoteHostsSectionFallback() {
  return (
    <section className="mb-8">
      <div className="mb-2.5 flex items-center justify-between">
        <span className={headingClasses}>Remote hosts</span>
        <Skeleton className="h-7 w-[104px] rounded-md" />
      </div>
      <Skeleton className="h-[62px] w-full rounded-xl" />
    </section>
  )
}

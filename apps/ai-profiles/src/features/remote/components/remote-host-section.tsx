import type { RemoteHost } from '@/lib/types'

import { Server, Terminal } from 'lucide-react'

import { StatusDot } from '@/design'
import { AppGlyph } from '@/features/profiles/components/app-glyph'
import { ManagedSidebarSwatch } from '@/features/profiles/components/managed-sidebar-swatch'
import { OutlinedSwatch } from '@/features/profiles/components/outlined-swatch'
import { SidebarProfileRow } from '@/features/profiles/components/sidebar-profile-row'

import { useRemoteAccounts } from '../api/use-remote'
import { remoteSelectionId } from '../lib/remote-selection'

type Props = {
  host: RemoteHost
  selectedId: string | null
  /** The sidebar's search text: profiles (and the host) are filtered by it. */
  query: string
  /** A profile's place in the ⌘1..⌘9 order, or -1 when it has none. */
  shortcutIndexFor: (id: string) => number
  onSelect: (id: string) => void
}

/**
 * One paired host in the sidebar: its name with a reachability dot, then its
 * profiles (its Claude accounts) as rows, each with the Claude glyph, its
 * color and its ⌘ number, as a profile on this Mac has. A host that doesn't answer keeps its section, so
 * it doesn't vanish from the sidebar while it's off, and says so.
 */
export function RemoteHostSection({ host, selectedId, query, shortcutIndexFor, onSelect }: Props) {
  const accounts = useRemoteAccounts(host.id)
  const needle = query.trim().toLowerCase()
  const hostMatches = needle.length === 0 || host.label.toLowerCase().includes(needle)
  const visible = (accounts.data ?? []).filter(
    (account) =>
      hostMatches ||
      account.name.toLowerCase().includes(needle) ||
      (account.account?.email ?? '').toLowerCase().includes(needle),
  )
  if (!hostMatches && visible.length === 0) {
    return null
  }
  const tone = accounts.isError ? 'danger' : accounts.isSuccess ? 'success' : 'neutral'

  return (
    <section aria-label={`Remote host ${host.label}`} className="flex flex-col gap-px">
      <div
        className="grid grid-cols-[13px_1fr_auto] items-center gap-1.5 px-2 pt-0.5 pb-1 text-[11px] font-medium text-muted-strong"
        title={
          accounts.isError
            ? `${host.label} isn't answering`
            : `${host.hostname} · ${host.lastGoodAddress ?? host.addresses[0]}`
        }
      >
        <Server aria-hidden strokeWidth={1.75} className="h-3 w-3" />
        <span className="truncate">{host.label}</span>
        <StatusDot tone={tone} />
      </div>
      {accounts.isError ? (
        <p className="px-2 pb-1 pl-[27px] text-[11.5px] text-muted">Offline</p>
      ) : (
        visible.map((account) => {
          const id = remoteSelectionId(host.id, account.name)
          return (
            <SidebarProfileRow
              key={id}
              name={account.name}
              // Remote profiles are Claude's for now: the server runs `claude`.
              glyph={<AppGlyph app="claude" size={13} />}
              swatch={
                host.profiles?.[account.name] ? (
                  <ManagedSidebarSwatch color={host.profiles[account.name].color} />
                ) : (
                  <OutlinedSwatch size={10} />
                )
              }
              shortcutIndex={shortcutIndexFor(id) >= 0 ? shortcutIndexFor(id) : undefined}
              // The CLI glyph local profiles show: a remote profile is Claude
              // Code on the host, with no desktop app of its own.
              trailing={
                <span className="flex items-center text-muted">
                  <Terminal aria-label="CLI" strokeWidth={1.75} className="h-3 w-3" />
                </span>
              }
              selected={selectedId === id}
              onSelect={() => onSelect(id)}
            />
          )
        })
      )}
    </section>
  )
}

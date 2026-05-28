import type { ReactNode } from 'react'
import type { DefaultEntry, Dependencies, Profile, SidebarEntry } from '@/lib/types'

import { Command } from 'cmdk'
import { CornerDownLeft, Download, Plus, Search, Settings as SettingsIcon, Terminal } from 'lucide-react'

import { cn, Kbd } from '@/design'
import { OutlinedSwatch } from '@/features/profiles/components/outlined-swatch'

type CommandPaletteProps = {
  open: boolean
  entries: Array<SidebarEntry>
  selectedId: string | null
  dependencies: Dependencies
  onClose: () => void
  onSwitch: (id: string) => void
  onLaunch: (profileId: string) => void
  onCopy: (profile: Profile) => void
  onCreate: () => void
  onSettings: () => void
  onImport: () => void
}

/**
 * cmdk-powered palette: each profile contributes flat rows ("Launch
 * desktop · {Name}", etc.) so filtering by name surfaces the action
 * directly. The active profile's rows sit at the top so ⏎ on first open
 * does the obvious thing.
 *
 * The ⌘K toggle binding lives in `app.tsx` next to the other global
 * shortcuts so palette-open and palette-closed states share one
 * registration site.
 */
export function CommandPalette({
  open,
  entries,
  selectedId,
  dependencies,
  onClose,
  onSwitch,
  onLaunch,
  onCopy,
  onCreate,
  onSettings,
  onImport,
}: CommandPaletteProps) {
  // Active entry's rows first so ⏎ on first open does the obvious thing.
  const ordered = [...entries].sort((entryA, entryB) => {
    const idA = entryA.kind === 'managed' ? entryA.profile.id : entryA.entry.id
    const idB = entryB.kind === 'managed' ? entryB.profile.id : entryB.entry.id
    if (idA === selectedId) {
      return -1
    }
    if (idB === selectedId) {
      return 1
    }
    return 0
  })

  function runAndClose(action: () => void) {
    action()
    onClose()
  }

  return (
    <Command.Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) {
          onClose()
        }
      }}
      label="Command palette"
      overlayClassName="fixed inset-0 z-40 bg-[rgba(26,24,21,0.18)] backdrop-blur-sm backdrop-saturate-150 dark:bg-[rgba(0,0,0,0.55)]"
      contentClassName="fixed left-1/2 top-[12vh] z-50 flex max-h-[calc(100%-128px)] w-[min(640px,calc(100%-64px))] -translate-x-1/2 flex-col overflow-hidden rounded-xl border border-[color:rgba(40,30,20,0.1)] bg-cream shadow-modal outline-none animate-[design-modal-in_0.22s_cubic-bezier(0.16,1,0.3,1)] dark:border-white/[0.06]"
    >
      <div className="relative border-b border-border-soft">
        <Search
          aria-hidden
          className="pointer-events-none absolute top-1/2 left-4 h-3.5 w-3.5 -translate-y-1/2 text-muted-strong"
          strokeWidth={2}
        />
        <Command.Input
          autoFocus
          placeholder="Type a command or search profiles…"
          className="w-full appearance-none border-0 bg-transparent py-3 pr-14 pl-10 font-sans text-[13px] text-ink outline-none placeholder:text-muted-strong"
        />
        <span className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2">
          <Kbd variant="subtle">esc</Kbd>
        </span>
      </div>
      <Command.List className="overflow-y-auto p-1 [&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:pt-2.5 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:font-mono [&_[cmdk-group-heading]]:text-eyebrow [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:tracking-[0.1em] [&_[cmdk-group-heading]]:text-muted-strong">
        <Command.Empty className="px-3 py-6 text-center text-meta text-muted-strong">No matches.</Command.Empty>
        {/* Entry rows: flat list (no per-entry heading), one row per surface + a switch row. */}
        {ordered.map((entry) =>
          entry.kind === 'managed' ? (
            <ProfileRows
              key={entry.profile.id}
              profile={entry.profile}
              onLaunch={() => runAndClose(() => onLaunch(entry.profile.id))}
              onCopy={() => runAndClose(() => onCopy(entry.profile))}
              onSwitch={() => runAndClose(() => onSwitch(entry.profile.id))}
            />
          ) : (
            <DefaultRow
              key={entry.entry.id}
              entry={entry.entry}
              onSwitch={() => runAndClose(() => onSwitch(entry.entry.id))}
            />
          ),
        )}
        <Command.Separator className="my-1 h-px bg-border-soft" />
        <Command.Group heading="Actions">
          <PaletteItem
            value="create-profile"
            keywords={['new', 'create', 'profile', 'add']}
            kbd="⌘N"
            onSelect={() => runAndClose(onCreate)}
            leading={<Plus className="h-3.5 w-3.5 text-muted" strokeWidth={1.85} />}
          >
            Create new profile
          </PaletteItem>
          <PaletteItem
            value="open-settings"
            keywords={['settings', 'preferences', 'config']}
            kbd="⌘,"
            onSelect={() => runAndClose(onSettings)}
            leading={<SettingsIcon className="h-3.5 w-3.5 text-muted" strokeWidth={1.85} />}
          >
            Open settings
          </PaletteItem>
          <PaletteItem
            value="detect-import"
            keywords={['detect', 'import', 'migration', 'existing']}
            kbd="⌘I"
            onSelect={() => runAndClose(onImport)}
            leading={<Download className="h-3.5 w-3.5 text-muted" strokeWidth={1.85} />}
            disabled={
              entries.some((entry) => entry.kind === 'managed') &&
              !dependencies.claudeAppInstalled &&
              !dependencies.claudeCliInstalled
            }
          >
            Detect and import…
          </PaletteItem>
        </Command.Group>
      </Command.List>
      <footer className="flex items-center justify-end gap-3 border-t border-border-soft px-3 py-2 text-[11px] text-muted-strong">
        <span className="inline-flex items-center gap-1.5">
          <Kbd variant="subtle">↑↓</Kbd>
          <span>select</span>
        </span>
        <span className="inline-flex items-center gap-1.5">
          <Kbd variant="subtle">
            <CornerDownLeft className="h-2.5 w-2.5" strokeWidth={2} />
          </Kbd>
          <span>run</span>
        </span>
        <span className="inline-flex items-center gap-1.5">
          <Kbd variant="subtle">⎋</Kbd>
          <span>close</span>
        </span>
      </footer>
    </Command.Dialog>
  )
}

type ProfileRowsProps = {
  profile: Profile
  onLaunch: () => void
  onCopy: () => void
  onSwitch: () => void
}

function ProfileRows({ profile, onLaunch, onCopy, onSwitch }: ProfileRowsProps) {
  return (
    <>
      {profile.surfaces.gui ? (
        <PaletteItem
          value={`launch-${profile.id}`}
          keywords={['launch', 'open', 'desktop', 'app', profile.name, profile.slug]}
          kbd="⏎"
          onSelect={onLaunch}
          leading={<ColorDot color={profile.color} />}
        >
          Launch desktop app <Separator /> {profile.name}
        </PaletteItem>
      ) : null}
      {profile.surfaces.cli ? (
        <PaletteItem
          value={`copy-${profile.id}`}
          keywords={['copy', 'cli', 'terminal', 'clipboard', profile.name, profile.slug]}
          kbd="⌘C"
          onSelect={onCopy}
          leading={<Terminal className="h-3.5 w-3.5 text-muted-strong" strokeWidth={1.85} />}
        >
          Copy <code className="font-mono text-ink">claude-{profile.slug}</code>
        </PaletteItem>
      ) : null}
      <PaletteItem
        value={`switch-${profile.id}`}
        keywords={['switch', 'select', profile.name, profile.slug]}
        kbd="→"
        onSelect={onSwitch}
        leading={<ColorDot color={profile.color} />}
      >
        Switch to {profile.name}
      </PaletteItem>
    </>
  )
}

type DefaultRowProps = {
  entry: DefaultEntry
  onSwitch: () => void
}

function DefaultRow({ entry, onSwitch }: DefaultRowProps) {
  return (
    <PaletteItem
      value={`switch-${entry.id}`}
      keywords={['switch', 'select', 'default', entry.name]}
      kbd="→"
      onSelect={onSwitch}
      leading={<OutlinedSwatch size={10} />}
    >
      Switch to {entry.name}
    </PaletteItem>
  )
}

type PaletteItemProps = {
  value: string
  keywords?: ReadonlyArray<string>
  kbd: string | ReactNode
  disabled?: boolean
  leading?: ReactNode
  onSelect: () => void
  children: ReactNode
}

function PaletteItem({ value, keywords, kbd, disabled, leading, onSelect, children }: PaletteItemProps) {
  return (
    <Command.Item
      value={value}
      keywords={keywords as Array<string> | undefined}
      onSelect={onSelect}
      disabled={disabled}
      className={cn(
        'grid grid-cols-[14px_1fr_auto] items-center gap-2.5 rounded-md px-3 py-1.5 text-[13px] tracking-[-0.003em] text-ink-soft cursor-pointer outline-none',
        'data-[selected=true]:bg-white/55 data-[selected=true]:text-ink data-[selected=true]:shadow-[inset_0_0_0_1px_rgba(229,224,210,0.55)]',
        'data-[disabled=true]:cursor-not-allowed data-[disabled=true]:opacity-50',
        'dark:data-[selected=true]:bg-white/[0.06] dark:data-[selected=true]:shadow-[inset_0_0_0_1px_rgba(255,255,255,0.06)]',
      )}
    >
      <span className="grid place-items-center">{leading}</span>
      <span className="truncate">{children}</span>
      <Kbd variant="subtle">{kbd}</Kbd>
    </Command.Item>
  )
}

function Separator() {
  return <span className="mx-1 text-border">·</span>
}

function ColorDot({ color }: { color: string }) {
  return (
    <span
      aria-hidden
      className="inline-block h-2.5 w-2.5 rounded-full shadow-[inset_0_0_0_1px_rgba(0,0,0,0.08),0_1px_1px_rgba(0,0,0,0.06)]"
      style={{ background: color }}
    />
  )
}

export type { CommandPaletteProps }

import { Plus } from 'lucide-react'

import { ariaKeyshortcutsFor, Button, Kbd } from '@/design'

import { EmptyStateBlobs } from './empty-state-blobs'

type Props = {
  onCreate: () => void
}

/**
 * Full-window empty state — used when the user has no profiles yet.
 * Rendered without a sidebar so the first-profile CTA owns the whole
 * window. PathBanner (if visible) sits above this screen, set by app.tsx.
 */
export function EmptyStateScreen({ onCreate }: Props) {
  return (
    <main className="flex h-full min-h-[600px] flex-1 flex-col items-center justify-center px-10 py-16 text-center">
      <EmptyStateBlobs />
      <div className="mb-2.5 font-mono text-[10.5px] font-medium uppercase tracking-[0.14em] text-muted-strong">
        No profiles yet
      </div>
      <h1 className="m-0 mb-3 text-h1 font-bold tracking-[-0.028em] text-ink leading-[1.1]">
        Create your first profile.
      </h1>
      <p className="m-0 mb-5 max-w-[380px] text-[13.5px] leading-[1.55] tracking-[-0.003em] text-muted">
        Each profile is one Claude account, with its own Desktop launcher and CLI wrapper. Logins, history, and chats
        stay isolated.
      </p>
      <Button
        variant="primary"
        size="md"
        leadingIcon={<Plus className="h-3.5 w-3.5" strokeWidth={2.25} />}
        trailingKbd={<Kbd variant="onOrange" shortcutId="open-create-profile" />}
        aria-keyshortcuts={ariaKeyshortcutsFor('open-create-profile')}
        onClick={onCreate}
      >
        New profile
      </Button>
    </main>
  )
}

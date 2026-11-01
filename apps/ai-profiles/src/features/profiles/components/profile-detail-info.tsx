import type { AppId } from '@/lib/app-registry'

import { Info } from '@phosphor-icons/react'

import { Popover, PopoverContent, PopoverTrigger } from '@/design/ui/popover'
import { appSpecs } from '@/lib/app-registry'

type Props = {
  app: AppId
  /**
   * The profile's wrapper command. Omitted for a stock install, which has no
   * wrapper — the paragraph describing one is then skipped rather than
   * claiming something untrue.
   */
  command?: string
}

/**
 * The ⓘ beside the profile name. It explains the profile, so it belongs
 * with the profile's identity rather than in the actions cluster.
 *
 * Single home for everything conceptual the pane used to repeat forever:
 * how the data is isolated, what the CLI wrapper and its config-directory
 * environment variable do, and the macOS caveat about the sign-in callback
 * landing in the wrong app.
 */
export function ProfileDetailInfo({ app, command }: Props) {
  const spec = appSpecs[app]
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label="About this profile"
          className="inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-full border-0 bg-transparent text-muted-strong outline-none transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:bg-cream-2 hover:text-ink focus-visible:ring-2 focus-visible:ring-orange/40"
        >
          <Info className="h-3.5 w-3.5" />
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-80 gap-2 text-meta leading-[1.5] text-muted">
        <p className="font-semibold text-ink">
          {command === undefined ? `About this ${spec.displayName} install` : 'How this profile is isolated'}
        </p>
        {command === undefined ? (
          <p>
            This is your stock {spec.displayName} install — the one you had before ai-profiles. It keeps its own logins,
            history, and chats, separate from any profile you create here.
          </p>
        ) : (
          <p>
            The launcher opens {spec.displayName} against its own user-data directory, so logins, history, and chats
            never mix with your other profiles.
          </p>
        )}
        {command === undefined ? null : (
          <p>
            <code className="font-mono text-ink-soft">{command}</code> is a wrapper script on your PATH that runs the
            stock <code className="font-mono text-ink-soft">{spec.cliBinary}</code> binary with{' '}
            <code className="font-mono text-ink-soft">{spec.cliConfigEnv}</code> pointed here.
          </p>
        )}
        <p>
          <span className="font-semibold text-ink-soft">Signing in:</span> with another {spec.displayName} window open,
          macOS can hand the sign-in callback to the wrong app. Quit the others for the first sign-in, or temporarily
          make Safari your default browser and switch back afterwards.
        </p>
      </PopoverContent>
    </Popover>
  )
}

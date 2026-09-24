import type { KeyboardEvent } from 'react'
import type { AppError, RemoteHost, WindowKey, WindowScreen } from '@/lib/types'

import { useEffect, useRef, useState } from 'react'

import { useQuery, useQueryClient } from '@tanstack/react-query'

import { Button } from '@/design'
import { sessionErrorMessage } from '@/features/profiles/components/session-error-message'
import { remoteWindowKeys, remoteWindowScreen } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

type Props = {
  host: RemoteHost
  account: string
  windowId: string
  /** Each time the screen is read, with whether the session registered. */
  onScreen?: (screen: WindowScreen) => void
}

/** Browser key names to tmux's. */
const namedKeys: Record<string, string> = {
  Enter: 'Enter',
  Escape: 'Escape',
  Tab: 'Tab',
  Backspace: 'BSpace',
  ArrowUp: 'Up',
  ArrowDown: 'Down',
  ArrowLeft: 'Left',
  ArrowRight: 'Right',
  Home: 'Home',
  End: 'End',
  PageUp: 'PageUp',
  PageDown: 'PageDown',
}

/** Buttons for the keys prompts want, for anyone who'd rather click. */
const keyButtons: ReadonlyArray<{ label: string; key: string; title: string }> = [
  { label: '↑', key: 'Up', title: 'Up' },
  { label: '↓', key: 'Down', title: 'Down' },
  { label: '⏎', key: 'Enter', title: 'Enter' },
  { label: 'Esc', key: 'Escape', title: 'Escape' },
  { label: 'Tab', key: 'Tab', title: 'Tab' },
  { label: '⌃C', key: 'C-c', title: 'Control-C' },
]

/** What a key press types into the window, or null for one it leaves alone. */
export function windowKeyFor(event: Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey'>) {
  if (event.metaKey) {
    return null
  }
  if (event.ctrlKey) {
    return event.key === 'c' || event.key === 'd' ? { key: `C-${event.key}` } : null
  }
  if (event.key === 'Tab' && event.shiftKey) {
    return { key: 'BTab' }
  }
  const named = namedKeys[event.key]
  if (named) {
    return { key: named }
  }
  if (!event.altKey && event.key.length === 1) {
    return event.key === ' ' ? { key: 'Space' } : { text: event.key }
  }
  return null
}

/**
 * A tmux window the host opened, live: its screen (refreshed every second)
 * and, while it has focus, the keyboard. For getting a session past
 * whatever Claude asks as it starts, without a terminal.
 */
export function RemoteWindow({ host, account, windowId, onScreen }: Props) {
  const queryClient = useQueryClient()
  const queryKey = queryKeys.remote.window(host.id, account, windowId)
  const screen = useQuery({
    queryKey,
    queryFn: () => remoteWindowScreen({ hostId: host.id, account, windowId }),
    refetchInterval: (query) => ((query.state.error as AppError | null)?.code === 'window_gone' ? false : 1000),
    retry: false,
  })
  // biome-ignore lint/correctness/useExhaustiveDependencies: report each screen once, not each render
  useEffect(() => {
    if (screen.data) {
      onScreen?.(screen.data)
    }
  }, [screen.data])
  // Keys go one request at a time, in the order they were pressed.
  const queue = useRef(Promise.resolve())
  const [sendError, setSendError] = useState<string | null>(null)

  function send(keys: Array<WindowKey>) {
    queue.current = queue.current.then(async () => {
      try {
        queryClient.setQueryData(queryKey, await remoteWindowKeys({ hostId: host.id, account, windowId, keys }))
        setSendError(null)
      } catch (caught) {
        setSendError(sessionErrorMessage(caught, 'That key did not get through.'))
      }
    })
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const key = windowKeyFor(event)
    if (key) {
      event.preventDefault()
      event.stopPropagation()
      send([key])
    }
  }

  const gone = (screen.error as AppError | null)?.code === 'window_gone'
  const text = screen.data?.text.replace(/\s+$/, '') ?? ''

  return (
    <div className="space-y-2">
      {gone ? (
        <p className="text-meta text-muted">The window has closed.</p>
      ) : (
        // A terminal: it takes the keyboard while focused.
        <div
          data-keeps-escape
          // biome-ignore lint/a11y/noNoninteractiveTabindex: a terminal takes the keyboard while focused
          tabIndex={0}
          role="application"
          aria-roledescription="terminal"
          aria-label={`Window ${windowId} on ${host.label}`}
          onKeyDown={handleKeyDown}
          className="max-h-[360px] overflow-auto rounded-md border border-border-soft bg-[#1d1b18] outline-none focus-visible:ring-2 focus-visible:ring-orange/50"
        >
          <pre
            className="m-0 whitespace-pre px-2.5 py-2 font-mono text-[11px] leading-[1.35] text-[#e9e4da]"
            style={screen.data ? { minWidth: `${screen.data.width}ch` } : undefined}
          >
            {screen.isPending ? '…' : text}
          </pre>
        </div>
      )}
      {gone ? null : (
        <div className="flex flex-wrap items-center gap-1">
          {keyButtons.map((button) => (
            <Button
              key={button.key}
              variant="ghost"
              size="sm"
              title={button.title}
              aria-label={`Press ${button.title}`}
              onClick={() => send([{ key: button.key }])}
            >
              {button.label}
            </Button>
          ))}
          <span className="ml-1 text-meta text-muted">Or click the screen and type.</span>
        </div>
      )}
      {sendError ? (
        <p role="alert" className="text-meta text-red">
          {sendError}
        </p>
      ) : screen.isError && !gone ? (
        <p role="alert" className="text-meta text-red">
          {sessionErrorMessage(screen.error, 'The window could not be read.')}
        </p>
      ) : null}
    </div>
  )
}

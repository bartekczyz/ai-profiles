import { Info } from 'lucide-react'

import { Popover, PopoverContent, PopoverTrigger } from '@/design/ui/popover'

export function DeepLinkInfo() {
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="inline-flex h-6 w-6 items-center justify-center rounded-full text-muted-foreground hover:bg-accent"
          aria-label="About logging in with multiple Claude apps open"
        >
          <Info className="h-3.5 w-3.5" />
        </button>
      </PopoverTrigger>
      <PopoverContent className="w-80 text-xs">
        <p className="font-semibold">Logging in to this profile</p>
        <p className="mt-2">
          If you have another Claude Desktop window open and try to log in here, macOS may route the OAuth callback (a{' '}
          <code>claude://</code> link) to the wrong app.
        </p>
        <p className="mt-2">
          Workaround: temporarily set Safari as your default browser before logging in, then switch back afterwards. Or
          quit other Claude windows for the first login.
        </p>
      </PopoverContent>
    </Popover>
  )
}

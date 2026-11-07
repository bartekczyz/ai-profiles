import type { ReactNode } from 'react'

import { QueryClientProvider, useQueryClient } from '@tanstack/react-query'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'

import { useDemoMode } from './use-demo-mode'

export { installDemoIpc } from './demo-ipc'

type DemoModeProps = {
  /**
   * The app.
   */
  children: ReactNode
}

/**
 * Dev only: runs the app on demo data while the Developer menu says so, on a
 * query client of the demo's own that is never persisted. The app remounts
 * into it, and back into its own client, from scratch. The query devtools
 * live here rather than in `QueryProvider`, so they stay out of screenshots.
 */
export function DemoMode({ children }: DemoModeProps) {
  const appClient = useQueryClient()
  const demoClient = useDemoMode()
  return (
    <QueryClientProvider key={demoClient ? 'demo' : 'app'} client={demoClient ?? appClient}>
      {children}
      {demoClient ? null : <ReactQueryDevtools initialIsOpen={false} buttonPosition="bottom-right" />}
    </QueryClientProvider>
  )
}

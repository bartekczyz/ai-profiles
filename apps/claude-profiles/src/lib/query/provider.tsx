import type { ReactNode } from 'react'

import { useState } from 'react'

import { createSyncStoragePersister } from '@tanstack/query-sync-storage-persister'
import { ReactQueryDevtools } from '@tanstack/react-query-devtools'
import { PersistQueryClientProvider } from '@tanstack/react-query-persist-client'

import { createQueryClient } from './client'

// How long a persisted snapshot is allowed to survive in storage. Usage
// windows reset on hourly/weekly cadences, but showing a day-old number with
// an "updated …" label still beats a blank card, and a background refresh
// corrects it on open. The whole persisted cache is dropped past this age.
const persistMaxAgeMs = 7 * 24 * 60 * 60 * 1000

// Bump when the persisted query shape changes so old snapshots are discarded
// rather than rehydrated into a newer reader.
const persistBuster = 'usage-v1'

type QueryProviderProps = {
  children: ReactNode
}

export function QueryProvider({ children }: QueryProviderProps) {
  const [client] = useState(() => createQueryClient())
  const [persister] = useState(() =>
    createSyncStoragePersister({ storage: window.localStorage, key: 'claude-profiles-query-cache' }),
  )
  return (
    <PersistQueryClientProvider
      client={client}
      persistOptions={{
        persister,
        maxAge: persistMaxAgeMs,
        buster: persistBuster,
        dehydrateOptions: {
          // Persist only the usage queries, and only while they hold data —
          // so the last good snapshot survives a restart even when the most
          // recent fetch errored (its data is retained, see Phase 1). Every
          // other query stays in-memory and re-fetches on launch as before.
          shouldDehydrateQuery: (query) => query.queryKey[0] === 'profile-usage' && query.state.data !== undefined,
        },
      }}
    >
      {children}
      {import.meta.env.DEV ? <ReactQueryDevtools initialIsOpen={false} buttonPosition="bottom-right" /> : null}
    </PersistQueryClientProvider>
  )
}

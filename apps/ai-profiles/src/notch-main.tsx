// SPDX-License-Identifier: MIT

import React from 'react'

import { createSyncStoragePersister } from '@tanstack/query-sync-storage-persister'
import { PersistQueryClientProvider } from '@tanstack/react-query-persist-client'
import ReactDOM from 'react-dom/client'

import { NotchHud } from '@/features/notch/components/notch-hud'
import { createQueryClient } from '@/lib/query/client'

import '@/features/notch/notch.css'

const client = createQueryClient()

// Its own key: the main window persists the profiles it has shown, the HUD
// every profile, and one cache written by both would drop the other's.
const persister = createSyncStoragePersister({ storage: window.localStorage, key: 'ai-profiles-notch-query-cache' })

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <PersistQueryClientProvider
      client={client}
      persistOptions={{
        persister,
        maxAge: 7 * 24 * 60 * 60 * 1000,
        buster: 'usage-v1',
        // The last reading shows in the folded notch after a restart, before
        // the panel has been opened.
        dehydrateOptions: {
          shouldDehydrateQuery: (query) => query.queryKey[0] === 'profile-usage' && query.state.data !== undefined,
        },
      }}
    >
      <NotchHud />
    </PersistQueryClientProvider>
  </React.StrictMode>,
)

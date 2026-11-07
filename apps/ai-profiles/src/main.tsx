// SPDX-License-Identifier: MIT

import React from 'react'

import { isTauri } from '@tauri-apps/api/core'
import ReactDOM from 'react-dom/client'

import { ThemeProvider, ToastProvider } from '@/design'
import { syncQueryFocusWithWindow } from '@/lib/query/focus'
import { QueryProvider } from '@/lib/query/provider'

import App from './app'

import './index.css'

// index.html marks the native shell before first paint; this repeats it in
// case Tauri's globals weren't there yet when that inline script ran. Without
// it the body keeps its opaque browser stand-in and hides the window material.
if (isTauri()) {
  document.documentElement.dataset.shell = 'native'
}

// Pause usage polling while the window is unfocused (see focus.ts).
void syncQueryFocusWithWindow()

// Dev builds can answer every command with fake data, for screenshots. It is
// dormant until switched on, and left out of release builds altogether.
if (import.meta.env.DEV) {
  const { installDemoIpc } = await import('@/features/demo-mode/demo-ipc')
  installDemoIpc()
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider defaultMode="system">
      <QueryProvider>
        <ToastProvider>
          <App />
        </ToastProvider>
      </QueryProvider>
    </ThemeProvider>
  </React.StrictMode>,
)

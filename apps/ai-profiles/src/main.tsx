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

let app = <App />
// Dev builds can show fake data in place of the user's, for screenshots: the
// Developer menu switches it on. Release builds leave it out altogether.
if (import.meta.env.DEV) {
  const { DemoMode, installDemoIpc } = await import('@/features/demo-mode/demo-mode')
  installDemoIpc()
  app = <DemoMode>{app}</DemoMode>
}

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider defaultMode="system">
      <QueryProvider>
        <ToastProvider>{app}</ToastProvider>
      </QueryProvider>
    </ThemeProvider>
  </React.StrictMode>,
)

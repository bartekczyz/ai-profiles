// SPDX-License-Identifier: MIT

import React from 'react'

import ReactDOM from 'react-dom/client'

import { ThemeProvider, ToastProvider } from '@/design'
import { syncQueryFocusWithWindow } from '@/lib/query/focus'
import { QueryProvider } from '@/lib/query/provider'

import App from './app'

import './index.css'

// Pause usage polling while the window is unfocused (see focus.ts).
void syncQueryFocusWithWindow()

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

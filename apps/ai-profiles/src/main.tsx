// SPDX-License-Identifier: MIT

import React from 'react'

import ReactDOM from 'react-dom/client'

import { ThemeProvider, ToastProvider } from '@/design'
import { QueryProvider } from '@/lib/query/provider'

import App from './app'

import './index.css'

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

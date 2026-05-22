import React from 'react'

import ReactDOM from 'react-dom/client'

import { ThemeProvider } from '@/design'

import App from './app'

import './index.css'

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider defaultMode="system">
      <App />
    </ThemeProvider>
  </React.StrictMode>,
)

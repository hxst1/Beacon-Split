import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import { invoke } from '@tauri-apps/api/core'

import { App } from './App'
import './styles/global.css'

/**
 * Sends anything fatal to the backend log.
 *
 * A release build has no inspector to open, and an uncaught render error
 * unmounts React's entire tree — a blank window that says nothing about why.
 * This is the difference between a bug that takes an evening to find and one
 * that names itself in the log.
 */
const report = (what: string, error: unknown): void => {
  const detail =
    error instanceof Error ? `${error.name}: ${error.message}\n${error.stack ?? ''}` : String(error)
  void invoke('report_frontend_error', { details: `${what} ${detail.slice(0, 900)}` })
}

window.addEventListener('error', (event) => report('window.error', event.error ?? event.message))
window.addEventListener('unhandledrejection', (event) => report('unhandled', event.reason))

const container = document.getElementById('root')
if (!container) throw new Error('missing #root element')

createRoot(container, {
  onUncaughtError: (error) => report('render.uncaught', error),
  onCaughtError: (error) => report('render.caught', error),
}).render(
  <StrictMode>
    <App />
  </StrictMode>,
)

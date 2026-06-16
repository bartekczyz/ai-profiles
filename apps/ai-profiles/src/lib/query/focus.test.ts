import { focusManager } from '@tanstack/react-query'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { syncQueryFocusWithWindow } from './focus'

type FocusHandler = (event: { payload: boolean }) => void

const handlers: Array<FocusHandler> = []
const isFocusedMock = vi.fn()

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    isFocused: isFocusedMock,
    onFocusChanged: (handler: FocusHandler) => {
      handlers.push(handler)
      return Promise.resolve(() => {})
    },
  }),
}))

afterEach(() => {
  handlers.length = 0
  focusManager.setFocused(undefined)
  vi.clearAllMocks()
})

describe('syncQueryFocusWithWindow', () => {
  it('seeds focusManager from the current window focus', async () => {
    isFocusedMock.mockResolvedValue(false)
    await syncQueryFocusWithWindow()
    expect(focusManager.isFocused()).toBe(false)
  })

  it('follows subsequent focus-change events', async () => {
    isFocusedMock.mockResolvedValue(true)
    await syncQueryFocusWithWindow()
    expect(focusManager.isFocused()).toBe(true)

    for (const handler of handlers) {
      handler({ payload: false })
    }
    expect(focusManager.isFocused()).toBe(false)

    for (const handler of handlers) {
      handler({ payload: true })
    }
    expect(focusManager.isFocused()).toBe(true)
  })
})

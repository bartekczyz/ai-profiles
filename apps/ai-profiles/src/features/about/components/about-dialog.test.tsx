import type { AppMetadata } from '@/lib/types'

import { screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import { getAppMetadata } from '@/lib/commands'
import { renderWithQuery } from '@/test/render-with-query'

import { AboutDialog } from './about-dialog'

vi.mock('@/lib/commands', () => ({ getAppMetadata: vi.fn(), openExternalUrl: vi.fn() }))

const metadata: AppMetadata = {
  name: 'ai-profiles',
  version: '1.1.0',
  description: '',
  authors: [],
  repository: null,
  homepage: null,
  license: null,
}

beforeEach(() => {
  vi.mocked(getAppMetadata).mockReset()
  vi.mocked(getAppMetadata).mockResolvedValue(metadata)
})

describe('AboutDialog', () => {
  it('opens the release notes from the version row', async () => {
    const onOpenWhatsNew = vi.fn()
    renderWithQuery(
      <ToastProvider>
        <AboutDialog open onClose={vi.fn()} onOpenWhatsNew={onOpenWhatsNew} />
      </ToastProvider>,
    )

    await userEvent.click(await screen.findByRole('button', { name: /what's new/i }))

    expect(onOpenWhatsNew).toHaveBeenCalledTimes(1)
  })
})

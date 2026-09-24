import type { RemoteHost } from '@/lib/types'

import { Suspense } from 'react'

import { screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ToastProvider } from '@/design'
import { copyToClipboard, remoteListHosts } from '@/lib/commands'
import { renderWithQuery } from '@/test/render-with-query'

import { RemoteHostsSection } from './remote-hosts-section'

vi.mock('@/lib/commands', async () => {
  const actual = await vi.importActual<typeof import('@/lib/commands')>('@/lib/commands')
  return {
    ...actual,
    remoteListHosts: vi.fn(),
    remoteHostInfo: vi.fn(() => new Promise(() => {})),
    copyToClipboard: vi.fn(async () => {}),
  }
})

const host: RemoteHost = {
  id: 'h1',
  label: 'xjopa1',
  hostname: 'xjopa1',
  addresses: ['100.110.48.97:7443'],
  fingerprint: '09:BB',
  clientId: 'c1',
  pairedAt: '2026-09-22T00:00:00Z',
  lastGoodAddress: null,
  profiles: {},
}

function renderSection() {
  renderWithQuery(
    <ToastProvider>
      <Suspense fallback={null}>
        <RemoteHostsSection />
      </Suspense>
    </ToastProvider>,
  )
}

beforeEach(() => {
  vi.mocked(copyToClipboard).mockClear()
})

describe('RemoteHostsSection — setup guide', () => {
  it('walks through setting up a host while none is paired, with commands to copy', async () => {
    vi.mocked(remoteListHosts).mockResolvedValue([])
    renderSection()
    const guide = (await screen.findByText('Run its setup')).closest('ol') as HTMLElement
    expect(within(guide).getAllByRole('listitem')).toHaveLength(4)
    const user = userEvent.setup()
    await user.click(within(guide).getByRole('button', { name: 'Copy ai-profiles-server setup' }))
    expect(copyToClipboard).toHaveBeenCalledWith('ai-profiles-server setup')
    expect(within(guide).getByRole('button', { name: 'Copied' })).toBeInTheDocument()
  })

  it('keeps it folded away once a host is paired', async () => {
    vi.mocked(remoteListHosts).mockResolvedValue([host])
    renderSection()
    const toggle = await screen.findByRole('button', { name: 'How to set up a host' })
    expect(toggle).toHaveAttribute('aria-expanded', 'false')
    expect(screen.queryByText('Run its setup')).toBeNull()
    await userEvent.setup().click(toggle)
    expect(screen.getByText('Run its setup')).toBeInTheDocument()
  })
})

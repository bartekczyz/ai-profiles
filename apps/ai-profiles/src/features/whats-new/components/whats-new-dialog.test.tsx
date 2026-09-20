import type { ChangelogRelease } from '../lib/changelog'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { WhatsNewDialog } from './whats-new-dialog'

const newer: ChangelogRelease = {
  version: '1.2.0',
  date: '2026-09-01',
  sections: [{ title: 'Fixed', items: [{ scope: 'usage', text: 'stop hammering dead credentials' }] }],
}

const older: ChangelogRelease = {
  version: '1.1.0',
  date: null,
  sections: [{ title: 'Added', items: [{ text: 'grouped profile rows' }] }],
}

describe('WhatsNewDialog', () => {
  it('lists the items of every release', () => {
    render(<WhatsNewDialog open releases={[newer, older]} onClose={vi.fn()} />)

    expect(screen.getByText('stop hammering dead credentials')).toBeInTheDocument()
    expect(screen.getByText('grouped profile rows')).toBeInTheDocument()
  })

  it('calls onClose from the close button', async () => {
    const onClose = vi.fn()
    render(<WhatsNewDialog open releases={[newer]} onClose={onClose} />)

    await userEvent.click(screen.getByRole('button', { name: /close/i }))

    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('renders nothing while closed', () => {
    render(<WhatsNewDialog open={false} releases={[newer]} onClose={vi.fn()} />)

    expect(screen.queryByText('stop hammering dead credentials')).not.toBeInTheDocument()
  })
})

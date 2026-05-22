import type { Profile } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { ProfileDetailHeader } from './profile-detail-header'

function fixture(overrides: Partial<Profile> = {}): Profile {
  return {
    id: '1',
    name: 'Acme Work',
    slug: 'acme-work',
    color: '#d97757',
    createdAt: '2026-05-20T12:00:00Z',
    lastUsedAt: null,

    surfaces: { gui: true, cli: true },
    ...overrides,
  }
}

describe('ProfileDetailHeader', () => {
  it('renders the profile name and slug', () => {
    render(<ProfileDetailHeader profile={fixture()} onEdit={vi.fn()} />)
    expect(screen.getByRole('heading', { name: 'Acme Work' })).toBeInTheDocument()
    expect(screen.getByText('acme-work')).toBeInTheDocument()
  })

  it('renders "Never used" when lastUsedAt is null', () => {
    render(<ProfileDetailHeader profile={fixture()} onEdit={vi.fn()} />)
    expect(screen.getByText('Never used')).toBeInTheDocument()
  })

  it('renders a relative timestamp when lastUsedAt is set', () => {
    const fiveMinutesAgo = new Date(Date.now() - 5 * 60_000).toISOString()
    render(<ProfileDetailHeader profile={fixture({ lastUsedAt: fiveMinutesAgo })} onEdit={vi.fn()} />)
    expect(screen.getByText(/Last used .* ago/)).toBeInTheDocument()
  })

  it('Edit button wears the ⌘E chip and fires onEdit', async () => {
    const onEdit = vi.fn()
    render(<ProfileDetailHeader profile={fixture()} onEdit={onEdit} />)
    const button = screen.getByRole('button', { name: /Edit/ })
    expect(button.textContent).toContain('⌘E')
    await userEvent.setup().click(button)
    expect(onEdit).toHaveBeenCalledTimes(1)
  })

  it('does not render the legacy "More actions" button (removed in Phase 15)', () => {
    render(<ProfileDetailHeader profile={fixture()} onEdit={vi.fn()} />)
    expect(screen.queryByRole('button', { name: /More actions/i })).not.toBeInTheDocument()
  })
})

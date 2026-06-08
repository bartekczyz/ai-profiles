import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { ProfileDetailHeader, ProfileSwatch } from './profile-detail-header'

describe('ProfileDetailHeader', () => {
  it('renders the profile name', () => {
    render(
      <ProfileDetailHeader
        name="Acme Work"
        swatch={<ProfileSwatch color="#d97757" />}
        subline={<span>acme-work</span>}
        onEdit={vi.fn()}
      />,
    )
    expect(screen.getByRole('heading', { name: 'Acme Work' })).toBeInTheDocument()
  })

  it('renders the subline content', () => {
    render(
      <ProfileDetailHeader
        name="Acme Work"
        swatch={<ProfileSwatch color="#d97757" />}
        subline={<span>acme-work</span>}
        onEdit={vi.fn()}
      />,
    )
    expect(screen.getByText('acme-work')).toBeInTheDocument()
  })

  it('renders "Never used" text when passed in subline', () => {
    render(
      <ProfileDetailHeader
        name="Acme Work"
        swatch={<ProfileSwatch color="#d97757" />}
        subline={<span>Never used</span>}
        onEdit={vi.fn()}
      />,
    )
    expect(screen.getByText('Never used')).toBeInTheDocument()
  })

  it('renders a relative timestamp when passed in subline', () => {
    render(
      <ProfileDetailHeader
        name="Acme Work"
        swatch={<ProfileSwatch color="#d97757" />}
        subline={<span>Last used 5 minutes ago</span>}
        onEdit={vi.fn()}
      />,
    )
    expect(screen.getByText(/Last used .* ago/)).toBeInTheDocument()
  })

  it('Edit button wears the ⌘E chip and fires onEdit', async () => {
    const onEdit = vi.fn()
    render(<ProfileDetailHeader name="Acme Work" swatch={<ProfileSwatch color="#d97757" />} onEdit={onEdit} />)
    const button = screen.getByRole('button', { name: /Edit/ })
    expect(button.textContent).toContain('⌘E')
    await userEvent.setup().click(button)
    expect(onEdit).toHaveBeenCalledTimes(1)
  })

  it('does not render Edit button when onEdit is not provided', () => {
    render(<ProfileDetailHeader name="Acme Work" swatch={<ProfileSwatch color="#d97757" />} />)
    expect(screen.queryByRole('button', { name: /Edit/ })).not.toBeInTheDocument()
  })

  it('does not render the legacy "More actions" button (removed in Phase 15)', () => {
    render(<ProfileDetailHeader name="Acme Work" swatch={<ProfileSwatch color="#d97757" />} onEdit={vi.fn()} />)
    expect(screen.queryByRole('button', { name: /More actions/i })).not.toBeInTheDocument()
  })
})

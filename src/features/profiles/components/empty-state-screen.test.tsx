import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { EmptyStateScreen } from './empty-state-screen'

describe('EmptyStateScreen', () => {
  it('renders the eyebrow, headline, and explanation copy', () => {
    render(<EmptyStateScreen onCreate={vi.fn()} />)
    expect(screen.getByText('No profiles yet')).toBeInTheDocument()
    expect(screen.getByRole('heading', { name: 'Create your first profile.' })).toBeInTheDocument()
    expect(screen.getByText(/Each profile is one Claude account/)).toBeInTheDocument()
  })

  it('primary CTA fires onCreate and carries a ⌘N chip', async () => {
    const onCreate = vi.fn()
    render(<EmptyStateScreen onCreate={onCreate} />)
    const cta = screen.getByRole('button', { name: /New profile/ })
    // The chip uses onOrange variant — text content includes ⌘N
    expect(cta.textContent).toContain('⌘N')
    await userEvent.setup().click(cta)
    expect(onCreate).toHaveBeenCalledTimes(1)
  })
})

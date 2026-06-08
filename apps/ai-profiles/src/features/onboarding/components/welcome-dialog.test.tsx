import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { WelcomeDialog } from './welcome-dialog'

describe('WelcomeDialog', () => {
  it('renders the welcome copy when open', () => {
    render(<WelcomeDialog open onContinue={vi.fn()} />)
    expect(screen.getByText(/Welcome to ai-profiles/i)).toBeInTheDocument()
    expect(screen.getByText(/Claude and Codex accounts/i)).toBeInTheDocument()
  })

  it('calls onContinue when Continue is clicked', async () => {
    const onContinue = vi.fn()
    render(<WelcomeDialog open onContinue={onContinue} />)
    await userEvent.setup().click(screen.getByRole('button', { name: 'Continue' }))
    expect(onContinue).toHaveBeenCalled()
  })

  it('does not render when open=false', () => {
    render(<WelcomeDialog open={false} onContinue={vi.fn()} />)
    expect(screen.queryByText(/Welcome to ai-profiles/i)).not.toBeInTheDocument()
  })
})

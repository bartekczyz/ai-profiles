import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { DeepLinkInfo } from './deep-link-info'

describe('DeepLinkInfo', () => {
  it('renders the trigger button with an aria-label', () => {
    render(<DeepLinkInfo />)
    expect(screen.getByRole('button', { name: /multiple Claude apps open/i })).toBeInTheDocument()
  })

  it('reveals the explanation when the trigger is clicked', async () => {
    render(<DeepLinkInfo />)
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: /multiple Claude apps open/i }))
    expect(await screen.findByText(/Logging in to this profile/i)).toBeInTheDocument()
    expect(screen.getByText(/Safari/)).toBeInTheDocument()
  })
})

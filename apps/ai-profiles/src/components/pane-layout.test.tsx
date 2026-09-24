import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { PaneLayout } from './pane-layout'

describe('PaneLayout', () => {
  it('keeps what was typed in the aside when the pane rerenders', async () => {
    const { rerender } = render(
      <PaneLayout header={<h1>Work</h1>} aside={<input aria-label="Search" />}>
        <p>Details</p>
      </PaneLayout>,
    )
    await userEvent.setup().type(screen.getByRole('textbox', { name: 'Search' }), 'parser')

    rerender(
      <PaneLayout header={<h1>Personal</h1>} aside={<input aria-label="Search" />} className="bg-white">
        <p>Other details</p>
      </PaneLayout>,
    )

    expect(screen.getByRole('textbox', { name: 'Search' })).toHaveValue('parser')
  })
})

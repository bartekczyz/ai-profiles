import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { Segmented } from './segmented'

describe('Segmented', () => {
  it('marks the active option with aria-checked', () => {
    render(
      <Segmented
        ariaLabel="theme"
        value="system"
        options={[
          { value: 'light', label: 'Light' },
          { value: 'system', label: 'System' },
          { value: 'dark', label: 'Dark' },
        ]}
        onChange={() => {}}
      />,
    )
    expect(screen.getByRole('radio', { name: 'System' })).toHaveAttribute('aria-checked', 'true')
    expect(screen.getByRole('radio', { name: 'Light' })).toHaveAttribute('aria-checked', 'false')
  })

  it('fires onChange with the selected value', async () => {
    const handleChange = vi.fn()
    render(
      <Segmented
        value="system"
        options={[
          { value: 'light', label: 'Light' },
          { value: 'dark', label: 'Dark' },
        ]}
        onChange={handleChange}
      />,
    )
    await userEvent.click(screen.getByRole('radio', { name: 'Dark' }))
    expect(handleChange).toHaveBeenCalledWith('dark')
  })
})

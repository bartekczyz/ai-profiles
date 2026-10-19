import { fireEvent, render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { toggleThreshold, UsageAlertThresholds } from './usage-alert-thresholds'

describe('toggleThreshold', () => {
  it('adds a threshold in order and removes a selected one', () => {
    expect(toggleThreshold([50, 90], 75)).toEqual([50, 75, 90])
    expect(toggleThreshold([50, 75, 90], 75)).toEqual([50, 90])
  })
})

describe('UsageAlertThresholds', () => {
  it('starts on the defaults and reports a toggled threshold', () => {
    const onChange = vi.fn()
    render(<UsageAlertThresholds value={null} onChange={onChange} />)

    expect(screen.getByRole('button', { name: '50%' })).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByRole('button', { name: '80%' })).toHaveAttribute('aria-pressed', 'false')

    fireEvent.click(screen.getByRole('button', { name: '80%' }))
    expect(onChange).toHaveBeenCalledWith([50, 75, 80, 90, 100])
  })
})

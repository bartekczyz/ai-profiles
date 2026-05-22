import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { ProfileDetailHintStrip } from './profile-detail-hint-strip'

describe('ProfileDetailHintStrip', () => {
  it('renders each global shortcut hint that maps to a working binding', () => {
    render(<ProfileDetailHintStrip />)
    const labels = ['open', 'copy', 'new', 'commands']
    for (const label of labels) {
      expect(screen.getByText(label)).toBeInTheDocument()
    }
  })

  it('does not advertise bindings we do not implement (navigate ↑↓, help ⌘?)', () => {
    render(<ProfileDetailHintStrip />)
    expect(screen.queryByText('navigate')).not.toBeInTheDocument()
    expect(screen.queryByText('help')).not.toBeInTheDocument()
  })

  it('renders the ⌘K kbd chip', () => {
    render(<ProfileDetailHintStrip />)
    expect(screen.getByText('⌘K')).toBeInTheDocument()
  })
})

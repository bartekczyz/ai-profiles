import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { Kbd, KbdGroup } from './kbd'

describe('Kbd', () => {
  it('renders content inside a <kbd> element', () => {
    render(<Kbd>⌘K</Kbd>)
    expect(screen.getByText('⌘K').tagName).toBe('KBD')
  })

  it('groups kbds with a wrapper span', () => {
    render(
      <KbdGroup>
        <Kbd>⌘</Kbd>
        <Kbd>K</Kbd>
      </KbdGroup>,
    )
    expect(screen.getByText('⌘')).toBeInTheDocument()
    expect(screen.getByText('K')).toBeInTheDocument()
  })
})

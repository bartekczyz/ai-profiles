import { render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { SidebarSearchInput } from './sidebar-search-input'

describe('SidebarSearchInput', () => {
  it('shows the ⌘F hint while the field is empty', () => {
    const { container } = render(<SidebarSearchInput value="" />)
    expect(container.querySelector('kbd')).toHaveTextContent('⌘F')
  })

  it('hides the ⌘F hint once there is a query', () => {
    const { container } = render(<SidebarSearchInput value="work" />)
    expect(container.querySelector('kbd')).toBeNull()
  })
})

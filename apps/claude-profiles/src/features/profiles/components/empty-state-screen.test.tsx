import type { Dependencies } from '@/lib/types'

import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { EmptyStateScreen } from './empty-state-screen'

const allInstalled: Dependencies = {
  claudeAppInstalled: true,
  claudeCliInstalled: true,
  localBinOnPath: true,
}

const noneInstalled: Dependencies = {
  claudeAppInstalled: false,
  claudeCliInstalled: false,
  localBinOnPath: false,
}

describe('EmptyStateScreen', () => {
  it('primary CTA fires onCreate when Claude is detected', async () => {
    const onCreate = vi.fn()
    render(<EmptyStateScreen dependencies={allInstalled} onCreate={onCreate} onRefresh={vi.fn()} />)
    await userEvent.setup().click(screen.getByRole('button', { name: /New profile/ }))
    expect(onCreate).toHaveBeenCalledTimes(1)
  })

  it('shows the not-detected state and calls onRefresh when neither Claude is installed', async () => {
    const onRefresh = vi.fn().mockResolvedValue(undefined)
    render(<EmptyStateScreen dependencies={noneInstalled} onCreate={vi.fn()} onRefresh={onRefresh} />)
    expect(screen.queryByRole('button', { name: /New profile/ })).not.toBeInTheDocument()
    await userEvent.setup().click(screen.getByRole('button', { name: /Check again/ }))
    expect(onRefresh).toHaveBeenCalledTimes(1)
  })

  it('falls back to the create CTA when only one surface is installed', () => {
    render(
      <EmptyStateScreen
        dependencies={{ ...allInstalled, claudeCliInstalled: false }}
        onCreate={vi.fn()}
        onRefresh={vi.fn()}
      />,
    )
    expect(screen.getByRole('button', { name: /New profile/ })).toBeInTheDocument()
  })
})

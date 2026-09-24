import { describe, expect, it } from 'vitest'

import { describeFailure } from './describe-failure'

describe('describeFailure', () => {
  it('tells a missing tool apart, with what to install', () => {
    expect(describeFailure({ kind: 'NotInstalled', message: 'Install the Codex CLI' })).toEqual({
      missingTool: true,
      message: 'Install the Codex CLI',
    })
  })

  it('treats any other failure as one to retry, with its message', () => {
    expect(describeFailure({ kind: 'Io', message: 'codex app-server exited' })).toEqual({
      missingTool: false,
      message: 'codex app-server exited',
    })
  })

  it('still has something to say about a failure with no message', () => {
    const described = describeFailure(undefined)
    expect(described.missingTool).toBe(false)
    expect(described.message).not.toBe('')
  })
})

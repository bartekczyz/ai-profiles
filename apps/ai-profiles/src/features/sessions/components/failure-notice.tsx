import { describeFailure } from '../lib/describe-failure'

type Props = {
  /**
   * Why a dialog's check or action failed.
   */
  failure: unknown
}

/**
 * Why a dialog's check or action failed. A missing tool is a state to
 * explain, said calmly; anything else is announced as an alert.
 */
export function FailureNotice({ failure }: Props) {
  const { missingTool, message } = describeFailure(failure)
  if (missingTool) {
    return <p className="text-body text-ink-soft">{message}</p>
  }
  return (
    <p role="alert" className="text-body text-red">
      {message}
    </p>
  )
}

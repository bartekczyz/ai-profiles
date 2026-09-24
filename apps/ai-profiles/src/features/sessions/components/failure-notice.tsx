import { extractErrorKind, extractErrorMessage } from '@/lib/extract-error-message'

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
  if (extractErrorKind(failure) === 'NotInstalled') {
    return <p className="text-body text-ink-soft">{extractErrorMessage(failure)}</p>
  }
  return (
    <p role="alert" className="text-body text-red">
      {extractErrorMessage(failure)}
    </p>
  )
}

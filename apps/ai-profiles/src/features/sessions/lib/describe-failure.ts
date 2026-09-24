import { extractErrorKind, extractErrorMessage } from '@/lib/extract-error-message'

/**
 * A failed listing, check or action, split the way the sessions UI shows it.
 */
export type DescribedFailure = {
  /**
   * Whether a tool the work needs isn't installed: a state to explain
   * calmly, not a failure to announce or retry.
   */
  missingTool: boolean
  /**
   * What to tell the user.
   */
  message: string
}

/**
 * Splits `failure` into a missing tool or anything else, with what to say
 * about it, so the panel and its dialogs tell the two apart one way.
 */
export function describeFailure(failure: unknown): DescribedFailure {
  return {
    missingTool: extractErrorKind(failure) === 'NotInstalled',
    message: extractErrorMessage(failure),
  }
}

import { extractErrorMessage } from '@/lib/extract-error-message'

/**
 * A backend error as a sentence for the Sessions panel and its dialogs.
 *
 * `AppError` serializes its `Display` text, which leads with the kind
 * ("validation error: …"). The sessions backend writes its refusals as
 * sentences meant for the user, so that lead is dropped.
 */
export function sessionErrorMessage(error: unknown, fallback?: string): string {
  return extractErrorMessage(error, fallback).replace(/^(validation error|not found|io error|json error): /, '')
}

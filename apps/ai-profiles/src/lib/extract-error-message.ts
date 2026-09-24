/**
 * Pull a human-readable message out of whatever the caller caught.
 *
 * The Rust side of this app serializes `AppError` as
 * `{ kind: 'Validation' | 'Io' | 'Json' | 'NotFound' | 'NotInstalled', message: string }`,
 * so Tauri's `invoke` rejection arrives as a plain object — not an
 * `Error`. Doing `String(caught)` produced `[object Object]` in dialogs;
 * this helper handles that case along with the usual `Error` / `string`
 * / `unknown` shapes.
 */
export function extractErrorMessage(error: unknown, fallback = 'Something went wrong.'): string {
  if (error instanceof Error) {
    return error.message
  }
  if (typeof error === 'string') {
    return error
  }
  if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message: unknown }).message
    if (typeof message === 'string' && message.length > 0) {
      return message
    }
  }
  return fallback
}

/**
 * The `kind` of an `AppError` from Rust, which tells a lasting state such as
 * `'NotInstalled'` from a failure worth retrying; `null` for anything else.
 */
export function extractErrorKind(error: unknown): string | null {
  if (error && typeof error === 'object' && 'kind' in error) {
    const kind = (error as { kind: unknown }).kind
    if (typeof kind === 'string') {
      return kind
    }
  }
  return null
}

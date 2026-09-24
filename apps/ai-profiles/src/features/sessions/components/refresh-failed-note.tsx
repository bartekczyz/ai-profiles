type Props = {
  /**
   * Whether a Retry is under way, which disables the link.
   */
  retrying: boolean
  /**
   * Refetches.
   */
  onRetry: () => void
}

/**
 * A quiet line above a list whose refresh failed: the rows below are the last
 * ones that loaded, and Retry tries again. Said politely, since nothing on
 * screen is wrong, only possibly out of date.
 */
export function RefreshFailedNote({ retrying, onRetry }: Props) {
  return (
    <p
      role="status"
      className="flex shrink-0 flex-wrap items-baseline gap-x-1.5 px-[13px] pb-2 text-[11.5px] text-muted-strong"
    >
      Couldn’t refresh — showing the last list.
      <button
        type="button"
        disabled={retrying}
        className="cursor-pointer text-ink-soft underline underline-offset-2 outline-none hover:text-ink focus-visible:ring-2 focus-visible:ring-orange/40 disabled:cursor-default disabled:opacity-60"
        onClick={onRetry}
      >
        Retry
      </button>
    </p>
  )
}

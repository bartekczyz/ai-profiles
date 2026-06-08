type Props = {
  size: number
}

/**
 * Grey outlined ring swatch. Used in two places:
 * - The sidebar row (10 px) — distinguishes the default entry from
 *   user-managed profiles' colour-filled dots.
 * - The detail-page header (44 px) — same visual language at scale.
 *
 * Drawn as a hollow ring rather than a filled circle so it reads as
 * "system / not-yours" without competing with the user-picked colours.
 */
export function OutlinedSwatch({ size }: Props) {
  const radiusClass = size <= 16 ? 'rounded-full' : 'rounded-xl'
  return (
    <span
      aria-hidden
      className={`inline-block shrink-0 ${radiusClass} border border-border-strong bg-transparent`}
      style={{ width: size, height: size }}
    />
  )
}

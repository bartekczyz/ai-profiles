import type { MeterLevel } from '../lib/profile-meters'

/** Segments per bar, as antiburn's HUD draws them. */
const segmentCount = 20

type Props = {
  /** Share of the limit used, 0-100, or null for an empty track. */
  percent: number | null
  /** Share of the period gone, 0-1: where an even pace would be by now. */
  elapsed: number | null
  level: MeterLevel
}

/**
 * A row of round LEDs, lit up to the share used, with a tick where an even
 * pace through the period would be. Usage left of the tick is behind pace;
 * usage past it is ahead.
 */
export function LedBar({ percent, elapsed, level }: Props) {
  const lit = percent === null ? 0 : Math.min(segmentCount, Math.round((Math.max(0, percent) / 100) * segmentCount))
  return (
    <div className="notch-led-bar" data-level={level} aria-hidden="true">
      {Array.from({ length: segmentCount }, (_, index) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: fixed positions, no state
        <span key={index} className={index < lit ? 'notch-led notch-led-lit' : 'notch-led'} />
      ))}
      {elapsed === null ? null : (
        <span className="notch-led-pace" style={{ left: `${Math.min(100, Math.max(0, elapsed * 100))}%` }} />
      )}
    </div>
  )
}

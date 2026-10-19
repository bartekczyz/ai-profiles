/** The percentages offered as usage alert thresholds. */
const thresholdOptions = [25, 50, 60, 75, 80, 90, 95, 100] as const

/** The thresholds a user starts with, mirroring the Rust default. */
const defaultThresholds = [50, 75, 90, 100]

type Props = {
  /** The stored thresholds, or null for the defaults. */
  value: Array<number> | null
  onChange: (thresholds: Array<number>) => void
}

/**
 * Pure: `thresholds` with `threshold` switched on or off, sorted.
 */
export function toggleThreshold(thresholds: ReadonlyArray<number>, threshold: number): Array<number> {
  const next = thresholds.includes(threshold)
    ? thresholds.filter((candidate) => candidate !== threshold)
    : [...thresholds, threshold]
  return next.sort((a, b) => a - b)
}

/**
 * The usage percentages that drop the notch panel open, as a row of toggle
 * chips. None selected turns the alerts off.
 */
export function UsageAlertThresholds({ value, onChange }: Props) {
  const selected = value ?? defaultThresholds
  return (
    <fieldset className="m-0 border-0 p-0">
      <legend className="sr-only">Usage alert thresholds</legend>
      <div className="flex flex-wrap gap-1.5">
        {thresholdOptions.map((threshold) => {
          const on = selected.includes(threshold)
          return (
            <button
              key={threshold}
              type="button"
              aria-pressed={on}
              onClick={() => onChange(toggleThreshold(selected, threshold))}
              className={`min-w-[44px] cursor-pointer rounded-md border px-2 py-1 font-mono text-[11px] transition-colors duration-(--duration-snap) ease-(--ease-natural) focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring ${
                on
                  ? 'border-transparent bg-(--app-accent) text-white'
                  : 'border-border bg-cream-2 text-muted-strong hover:text-ink'
              }`}
            >
              {threshold}%
            </button>
          )
        })}
      </div>
    </fieldset>
  )
}

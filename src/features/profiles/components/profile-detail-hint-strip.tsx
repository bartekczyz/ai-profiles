import { Kbd } from '@/design'

type Hint = {
  label: string
  keys: ReadonlyArray<string>
}

// Each hint here MUST correspond to a registered shortcut that fires in the
// detail/global scope. Don't add `navigate ↑↓` (no global arrow keys) or
// `help ⌘?` (no help cheatsheet exists) — advertising a binding that does
// nothing is worse than no hint at all.
const HINTS: ReadonlyArray<Hint> = [
  { label: 'open', keys: ['⏎'] },
  { label: 'copy', keys: ['⌘C'] },
  { label: 'new', keys: ['⌘N'] },
  { label: 'commands', keys: ['⌘K'] },
]

/**
 * Bottom hint strip — a thin cream-tinted bar reminding the user of the
 * global keyboard shortcuts. Sits below the detail content, above the
 * detail pane's bottom edge.
 */
export function ProfileDetailHintStrip() {
  return (
    <div className="flex shrink-0 items-center justify-center gap-4 border-t border-border-soft bg-cream/55 px-6 py-3 text-[11.5px] text-muted-strong">
      {HINTS.map((hint, index) => (
        <span key={hint.label} className="inline-flex items-center gap-1.5">
          <span className="inline-flex items-center gap-0.5">
            {hint.keys.map((key) => (
              <Kbd key={key} variant="subtle">
                {key}
              </Kbd>
            ))}
          </span>
          <span>{hint.label}</span>
          {index < HINTS.length - 1 ? (
            <span aria-hidden className="ml-3 text-border">
              ·
            </span>
          ) : null}
        </span>
      ))}
    </div>
  )
}

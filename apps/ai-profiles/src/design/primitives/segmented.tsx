import type { ReactNode } from 'react'

import { cn } from '@/design/lib/cn'

type SegmentedOption<Value extends string> = {
  value: Value
  label: ReactNode
  icon?: ReactNode
  ariaLabel?: string
}

type SegmentedProps<Value extends string> = {
  options: ReadonlyArray<SegmentedOption<Value>>
  value: Value
  ariaLabel?: string
  className?: string
  onChange: (value: Value) => void
}

export function Segmented<Value extends string>({
  options,
  value,
  ariaLabel,
  className,
  onChange,
}: SegmentedProps<Value>) {
  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className={cn(
        'inline-flex items-center gap-[2px] p-[3px] rounded-[9px] border border-border bg-cream-2',
        className,
      )}
    >
      {options.map((option) => {
        const isActive = option.value === value
        return (
          // biome-ignore lint/a11y/useSemanticElements: segmented control needs button semantics with single-select aria — radio inputs don't allow icon-leading layout
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={isActive}
            aria-label={option.ariaLabel}
            onClick={() => onChange(option.value)}
            className={cn(
              'inline-flex items-center gap-1.5 h-7 px-3.5 rounded-md text-[12.5px] font-medium tracking-[-0.005em] cursor-pointer transition-all duration-(--duration-snap) ease-(--ease-natural)',
              isActive
                ? 'bg-cream text-ink shadow-[0_1px_2px_rgba(0,0,0,0.08),inset_0_0_0_1px_var(--color-border)]'
                : 'text-muted hover:text-ink',
            )}
          >
            {option.icon ? (
              <span className="inline-flex h-[13px] w-[13px] items-center justify-center">{option.icon}</span>
            ) : null}
            <span>{option.label}</span>
          </button>
        )
      })}
    </div>
  )
}

export type { SegmentedOption, SegmentedProps }

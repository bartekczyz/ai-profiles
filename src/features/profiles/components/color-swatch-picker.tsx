import { isValidHexColor, presetColors } from '@/lib/colors'

type Props = {
  value: string
  onChange: (color: string) => void
}

/**
 * Eight-swatch picker matching the prototype. Each swatch is 26×26 with
 * a subtle inner shadow; the selected one wears an outer ink ring.
 */
export function ColorSwatchPicker({ value, onChange }: Props) {
  return (
    <div className="flex flex-wrap items-center gap-2.5">
      {presetColors.map((preset) => {
        const selected = value.toLowerCase() === preset.toLowerCase()
        return (
          <button
            key={preset}
            type="button"
            aria-label={`Pick color ${preset}`}
            aria-pressed={selected}
            onClick={() => onChange(preset)}
            data-selected={selected ? 'true' : 'false'}
            className="relative h-[26px] w-[26px] cursor-pointer rounded-full border-0 p-0 shadow-[inset_0_0_0_1px_rgba(0,0,0,0.08)] outline-none transition-transform duration-(--duration-snap) ease-(--ease-natural) hover:scale-[1.08] data-[selected=true]:after:absolute data-[selected=true]:after:-inset-1 data-[selected=true]:after:rounded-full data-[selected=true]:after:border-[1.5px] data-[selected=true]:after:border-ink data-[selected=true]:after:content-['']"
            style={{ background: preset }}
          />
        )
      })}
      <input
        type="text"
        value={value}
        placeholder={presetColors[0]}
        onChange={(event) => onChange(event.target.value)}
        aria-label="Custom hex color"
        aria-invalid={!isValidHexColor(value)}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        className="ml-1 h-8 w-[110px] appearance-none rounded-md border border-border bg-white px-2 font-mono text-[12px] text-ink outline-none transition-colors focus:border-orange aria-[invalid=true]:border-red dark:bg-cream-2"
      />
    </div>
  )
}

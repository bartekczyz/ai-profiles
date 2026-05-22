import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/label'
import { isValidHexColor, presetColors } from '@/lib/colors'

type Props = {
  value: string
  onChange: (color: string) => void
}

export function ColorSwatchPicker({ value, onChange }: Props) {
  return (
    <div className="space-y-2">
      <Label>Color</Label>
      <div className="flex flex-wrap gap-2">
        {presetColors.map((preset) => {
          const selected = value.toLowerCase() === preset.toLowerCase()
          return (
            <button
              key={preset}
              type="button"
              aria-label={`Pick color ${preset}`}
              onClick={() => onChange(preset)}
              className={
                selected ? 'h-8 w-8 rounded-full ring-2 ring-offset-2 ring-foreground' : 'h-8 w-8 rounded-full'
              }
              style={{ background: preset }}
            />
          )
        })}
      </div>
      <Input
        type="text"
        value={value}
        placeholder={presetColors[0]}
        onChange={(event) => onChange(event.target.value)}
        aria-invalid={!isValidHexColor(value)}
        className="font-mono"
      />
    </div>
  )
}

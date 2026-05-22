import type { Profile } from '@/lib/types'

import { useEffect, useState } from 'react'

import { Button } from '@/design/ui/button'
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/design/ui/dialog'
import { Input } from '@/design/ui/input'
import { Label } from '@/design/ui/label'
import { isValidHexColor } from '@/lib/colors'

import { ColorSwatchPicker } from './color-swatch-picker'

type Props = {
  open: boolean
  profile: Profile
  submitting?: boolean
  onClose: () => void
  onSave: (input: { name: string; color: string }) => Promise<void>
}

export function EditProfileModal({ open, profile, submitting, onClose, onSave }: Props) {
  const [name, setName] = useState(profile.name)
  const [color, setColor] = useState(profile.color)
  const [error, setError] = useState<string | null>(null)

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset only when the profile identity changes
  useEffect(() => {
    setName(profile.name)
    setColor(profile.color)
    setError(null)
  }, [profile.id])

  const dirty = name.trim() !== profile.name || color.toLowerCase() !== profile.color.toLowerCase()
  const canSubmit = name.trim().length > 0 && isValidHexColor(color) && dirty

  async function handleSubmit() {
    setError(null)
    try {
      await onSave({ name: name.trim(), color })
      onClose()
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    }
  }

  return (
    <Dialog open={open} onOpenChange={(next) => (next ? null : onClose())}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit profile</DialogTitle>
        </DialogHeader>
        <div className="space-y-4 py-2">
          <div className="space-y-2">
            <Label htmlFor="edit-name">Name</Label>
            <Input id="edit-name" value={name} onChange={(event) => setName(event.target.value)} autoFocus />
          </div>
          <ColorSwatchPicker value={color} onChange={setColor} />
          {error ? <p className="text-sm text-red">{error}</p> : null}
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose} disabled={submitting}>
            Cancel
          </Button>
          <Button onClick={handleSubmit} disabled={!canSubmit || submitting}>
            Save
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

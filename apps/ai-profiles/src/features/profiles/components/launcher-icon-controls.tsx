import type { AppId } from '@/lib/types'
import type { IconChange } from '../lib/launcher-icon'

import { useRef, useState } from 'react'

import { Button, cn, useToast } from '@/design'
import { checkCustomIcon } from '@/lib/commands'
import { extractErrorMessage } from '@/lib/extract-error-message'

import { useGeneratedIconPreview } from '../api/use-custom-icon'
import { launcherIconControls, pngDataUrl, resetIcon } from '../lib/launcher-icon'
import { DockPreview } from './dock-preview'

export type LauncherIconField = {
  /**
   * The icon the launcher has, or will have once the form is saved, as a data
   * URL; `null` while it is still being rendered.
   */
  icon: string | null
  /**
   * Whether to offer removing the user's icon.
   */
  canRemove: boolean
  /**
   * Takes the PNG the user picked.
   */
  onChoose: (file: File) => void
  /**
   * Removes the user's icon, for the generated one.
   */
  onRemove: () => void
}

/**
 * The launcher-icon change made in the edit form of profile `profileId`, and
 * the controls' field for making it. `customIcon` is the profile's own icon
 * (`null` for none, `undefined` until that is known); `app` and `color` are
 * what the generated icon is drawn from, the color as picked in the form. The
 * generated icon is rendered only while the form is `open`. A change belongs to
 * the profile it was made for: opening another profile starts from none.
 */
export function useLauncherIconChange({
  profileId,
  app,
  color,
  customIcon,
  open,
}: {
  profileId: string
  app: AppId
  color: string
  customIcon: string | null | undefined
  open: boolean
}): { change: IconChange | null; field: LauncherIconField; clear: () => void } {
  const toast = useToast()
  const generated = useGeneratedIconPreview(app, color, open)
  const [made, setMade] = useState<{ profileId: string; change: IconChange | null }>({ profileId, change: null })
  // Counts the changes made, so a picked file that is read only after a later
  // change (Reset, another pick) doesn't overwrite it.
  const changes = useRef(0)
  const change = made.profileId === profileId ? made.change : null
  const set = (next: IconChange | null) => {
    changes.current += 1
    setMade({ profileId, change: next })
  }
  return {
    change,
    field: {
      ...launcherIconControls(customIcon, change, generated),
      // Checked at once, so an image that can't be an icon is turned away with
      // why, rather than shown and refused only on save.
      onChoose: async (file) => {
        const pick = ++changes.current
        try {
          const png = await file.arrayBuffer()
          await checkCustomIcon(new Uint8Array(png))
          if (pick === changes.current) {
            set({ kind: 'custom', file, preview: pngDataUrl(png) ?? '' })
          }
        } catch (caught) {
          toast.error('Could not use this image.', extractErrorMessage(caught))
        }
      },
      onRemove: () => set(resetIcon(customIcon != null)),
    },
    clear: () => set(null),
  }
}

type Props = LauncherIconField & {
  /**
   * Set while there is no desktop launcher for the icon to be on.
   */
  disabled: boolean
}

/**
 * The launcher-icon controls, set under the Dock icon option's text and lined
 * up with it: a preview of the icon in the Dock, and the buttons that change
 * it. The icon is what that option's Dock tile shows, and what Finder and
 * Launchpad show either way. A picked image shows in the preview at once, but
 * is applied only on save.
 */
export function LauncherIconControls({ icon, canRemove, disabled, onChoose, onRemove }: Props) {
  const input = useRef<HTMLInputElement>(null)
  return (
    // Lined up with the option's text: past its padding (40px), checkbox (16px)
    // and gap (12px) on the left, and clear of its info button (44px) on the
    // right, so the preview and the buttons under it are as wide as the text.
    <div className={cn('flex flex-col gap-2 pr-11 pb-3 pl-[68px]', disabled && 'opacity-60')}>
      <DockPreview icon={icon} />
      {/* One row, never wrapped: the two buttons are a pair. They fill the
          preview's width, each keeping its own and sharing what is left, as
          equal halves would be too narrow for "Choose custom icon". */}
      <div className="flex items-center gap-2">
        <Button size="sm" className="grow" disabled={disabled} onClick={() => input.current?.click()}>
          Choose custom icon
        </Button>
        {canRemove ? (
          <Button variant="ghost" size="sm" className="grow" disabled={disabled} onClick={onRemove}>
            Reset to default
          </Button>
        ) : null}
      </div>
      <input
        ref={input}
        hidden
        type="file"
        accept="image/png"
        aria-label="Launcher icon image"
        onChange={(event) => {
          const file = event.target.files?.[0]
          // Cleared, so picking the same file again still reports a change.
          event.target.value = ''
          if (file !== undefined) {
            onChoose(file)
          }
        }}
      />
    </div>
  )
}

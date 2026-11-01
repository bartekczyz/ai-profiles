import type { AppId } from '@/lib/types'
import type { IconChange } from '../lib/launcher-icon'

import { useEffect } from 'react'

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { isValidHexColor, presetColors } from '@/lib/colors'
import { clearCustomIcon, customIcon, generatedIconPreview, setCustomIcon } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

import { pngDataUrl } from '../lib/launcher-icon'

/**
 * The image profile `profileId` picked for its launcher, as a data URL, or
 * `null` when it has none; `undefined` until that is known, and while not
 * `enabled` or for `null`, which ask for nothing.
 */
export function useCustomIcon(profileId: string | null, enabled: boolean): string | null | undefined {
  const { data } = useQuery({
    queryKey: queryKeys.profiles.customIcon(profileId ?? ''),
    queryFn: async () => pngDataUrl(await customIcon(profileId ?? '')),
    enabled: enabled && profileId !== null,
  })
  return data
}

/**
 * The icon a launcher of `app` gets in `color` without a picked image, as a
 * data URL; `null` while it renders, while not `enabled`, and while no app is
 * chosen (`''`). The previous
 * color's icon of the same app stays up meanwhile, so the preview doesn't
 * flicker as swatches are clicked through, and while a hex color is half
 * typed, which there is no icon for.
 */
export function useGeneratedIconPreview(app: AppId | '', color: string, enabled: boolean): string | null {
  const queryClient = useQueryClient()
  const { data = null, isPlaceholderData } = useQuery({
    queryKey: queryKeys.generatedIconPreview(app, color),
    queryFn: async () => pngDataUrl(await generatedIconPreview(app as AppId, color)),
    placeholderData: (previous, previousQuery) => (previousQuery?.queryKey[1] === app ? previous : undefined),
    enabled: enabled && app !== '' && isValidHexColor(color),
  })
  const shown = data !== null && !isPlaceholderData
  // Once the chosen colour's icon is on screen, draw the other presets one at
  // a time, so picking one later shows its icon at once. Only after, and only
  // one by one: each render is real work on the backend, and a batch fired up
  // front would queue ahead of the icon the form is waiting for.
  useEffect(() => {
    if (!shown || app === '') {
      return
    }
    let cancelled = false
    void (async () => {
      for (const preset of presetColors) {
        if (cancelled) {
          return
        }
        await queryClient.prefetchQuery({
          queryKey: queryKeys.generatedIconPreview(app, preset),
          queryFn: async () => pngDataUrl(await generatedIconPreview(app, preset)),
        })
      }
    })()
    return () => {
      cancelled = true
    }
  }, [app, shown, queryClient])
  return data
}

/**
 * Applies a launcher-icon change from the edit form; `null`, for none, does
 * nothing. Either way the backend rebuilds the launcher, and it checks the
 * image first, so what is wrong with one (not square, too small) is the error
 * this rejects with.
 */
export function useApplyIconChange(): (input: { profileId: string; change: IconChange | null }) => Promise<void> {
  const queryClient = useQueryClient()
  const mutation = useMutation({
    mutationFn: async ({ profileId, change }: { profileId: string; change: IconChange }) => {
      if (change.kind === 'generated') {
        return clearCustomIcon(profileId)
      }
      return setCustomIcon(profileId, new Uint8Array(await change.file.arrayBuffer()))
    },
    onSuccess: (_void, { profileId, change }) => {
      queryClient.setQueryData(
        queryKeys.profiles.customIcon(profileId),
        change.kind === 'custom' ? change.preview : null,
      )
    },
  })
  return async ({ profileId, change }) => {
    if (change !== null) {
      await mutation.mutateAsync({ profileId, change })
    }
  }
}

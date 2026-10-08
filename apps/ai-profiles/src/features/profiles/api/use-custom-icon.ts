import type { AppId } from '@/lib/types'
import type { IconChange } from '../lib/launcher-icon'

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { isValidHexColor } from '@/lib/colors'
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
 * data URL; `null` while it renders, and while not `enabled`. The previous
 * color's icon of the same app stays up meanwhile, so the preview doesn't
 * flicker as swatches are clicked through, and while a hex color is half
 * typed, which there is no icon for.
 */
export function useGeneratedIconPreview(app: AppId, color: string, enabled: boolean): string | null {
  const { data = null } = useQuery({
    queryKey: queryKeys.generatedIconPreview(app, color),
    queryFn: async () => pngDataUrl(await generatedIconPreview(app, color)),
    placeholderData: (previous, previousQuery) => (previousQuery?.queryKey[1] === app ? previous : undefined),
    enabled: enabled && isValidHexColor(color),
  })
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

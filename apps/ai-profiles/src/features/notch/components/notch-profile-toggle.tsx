import type { AppState, AppStatePatch } from '@/lib/types'

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { computeOptimisticAppState } from '@/lib/app-state/use-app-state'
import { loadAppState, updateAppState } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * Whether profile `profileId` shows in the notch, and the way to change it.
 *
 * Reads the app state without suspending: the controls sit in the usage
 * card's header, and a missing reading only hides them, never the card.
 */
function useNotchProfile(profileId: string) {
  const queryClient = useQueryClient()
  const { data } = useQuery({ queryKey: queryKeys.appState, queryFn: () => loadAppState() })
  const mutation = useMutation({
    mutationFn: (patch: AppStatePatch) => updateAppState(patch),
    onMutate: (patch) => {
      const previous = queryClient.getQueryData<AppState>(queryKeys.appState)
      if (previous) {
        queryClient.setQueryData(queryKeys.appState, computeOptimisticAppState(previous, patch))
      }
      return { previous }
    },
    onError: (_error, _patch, context) => {
      if (context?.previous) {
        queryClient.setQueryData(queryKeys.appState, context.previous)
      }
    },
    onSuccess: (next) => {
      queryClient.setQueryData(queryKeys.appState, next)
    },
  })
  if (!data) {
    return null
  }
  return {
    shown: !data.notchHiddenProfiles.includes(profileId),
    hudOff: data.notchHudOff,
    setShown: (shown: boolean) => mutation.mutate({ notchProfile: { profileId, shown } }),
  }
}

/**
 * The usage card's switch for showing this profile in the notch. Absent
 * while the notch HUD is off as a whole.
 */
export function NotchProfileToggle({ profileId }: { profileId: string }) {
  const notch = useNotchProfile(profileId)
  if (!notch || notch.hudOff) {
    return null
  }
  return (
    <button
      type="button"
      role="switch"
      aria-checked={notch.shown}
      onClick={() => notch.setShown(!notch.shown)}
      className="group inline-flex min-h-[22px] cursor-pointer items-center gap-1.5 rounded px-1 font-mono text-mono text-muted-strong transition-colors duration-(--duration-snap) ease-(--ease-natural) hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
    >
      In notch
      <span
        aria-hidden
        className={`relative inline-block h-[14px] w-[24px] rounded-full transition-colors duration-(--duration-snap) ease-(--ease-natural) ${notch.shown ? 'bg-(--app-accent)' : 'bg-ink/20'}`}
      >
        <span
          className={`absolute top-[2px] left-[2px] h-[10px] w-[10px] rounded-full bg-white shadow-sm transition-transform duration-(--duration-snap) ease-(--ease-natural) ${notch.shown ? 'translate-x-[10px]' : ''}`}
        />
      </span>
    </button>
  )
}

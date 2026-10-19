import type { SegmentedOption } from '@/design'
import type { AppState } from '@/lib/types'

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { Segmented, TooltipBubble } from '@/design'
import { computeOptimisticAppState } from '@/lib/app-state/use-app-state'
import { loadAppState, updateAppState } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

type WeekOption = '5' | '7'

const weekOptions: ReadonlyArray<SegmentedOption<WeekOption>> = [
  { value: '5', label: '5d', ariaLabel: 'Five-day week, Monday to Friday' },
  { value: '7', label: '7d', ariaLabel: 'Seven-day week' },
]

/**
 * Profile `profileId`'s working week, and the way to change it.
 *
 * Reads the app state without suspending: the picker sits in the usage
 * card's header, and a missing reading only hides the picker, never the card.
 */
function useWorkingWeek(profileId: string) {
  const queryClient = useQueryClient()
  const { data } = useQuery({ queryKey: queryKeys.appState, queryFn: () => loadAppState() })
  const mutation = useMutation({
    mutationFn: (days: 5 | 7) => updateAppState({ workingWeek: { profileId, days } }),
    onMutate: (days) => {
      const previous = queryClient.getQueryData<AppState>(queryKeys.appState)
      if (previous) {
        queryClient.setQueryData(
          queryKeys.appState,
          computeOptimisticAppState(previous, { workingWeek: { profileId, days } }),
        )
      }
      return { previous }
    },
    onError: (_error, _days, context) => {
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
    week: (data.fiveDayWeekProfiles.includes(profileId) ? '5' : '7') as WeekOption,
    hudOff: data.notchHudOff,
    setWeek: (week: WeekOption) => mutation.mutate(week === '5' ? 5 : 7),
  }
}

/**
 * The usage card's picker for whether this profile's weekly pace in the
 * notch counts five days or seven. Absent while the notch HUD is off.
 */
export function WorkingWeekPicker({ profileId }: { profileId: string }) {
  const working = useWorkingWeek(profileId)
  if (!working || working.hudOff) {
    return null
  }
  return (
    <span className="group relative inline-flex">
      <Segmented<WeekOption>
        size="small"
        ariaLabel="Working week"
        options={weekOptions}
        value={working.week}
        onChange={working.setWeek}
      />
      <TooltipBubble>Working week for the weekly pace</TooltipBubble>
    </span>
  )
}

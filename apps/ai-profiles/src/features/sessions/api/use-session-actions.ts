import type { SessionAction } from '@/lib/types'

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { archiveSession, checkSessionAction, restoreSession } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * What a session action is asked to do.
 */
type SessionActionInput = {
  /**
   * The session to act on.
   */
  sessionId: string
  /**
   * What to do to it.
   */
  action: SessionAction
  /**
   * Quit the desktop app in the way first.
   */
  quitApp: boolean
}

/**
 * What stands between a session and an action: a blocker, or the desktop app
 * that has to quit first. Looked up whenever it is asked for and again on
 * every window focus, as the user may close a terminal or quit the app
 * themselves while the question is open.
 */
export function useSessionActionCheck(profileId: string, sessionId: string, action: SessionAction) {
  return useQuery({
    queryKey: queryKeys.sessionActionCheck(profileId, sessionId, action),
    queryFn: () => checkSessionAction(profileId, sessionId, action),
    staleTime: 0,
    gcTime: 0,
    refetchOnWindowFocus: 'always',
  })
}

/**
 * Archives or restores one of a profile's sessions. Every profile's list is
 * refetched afterwards, whether it worked or not (a desktop app may have quit
 * on the way), as a session's files can sit in another profile's config dir.
 * The refetch isn't waited for, so the action settles as soon as it is done.
 */
export function useSessionAction(profileId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ sessionId, action, quitApp }: SessionActionInput) =>
      action === 'archive'
        ? archiveSession(profileId, sessionId, quitApp)
        : restoreSession(profileId, sessionId, quitApp),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.sessions.all })
    },
  })
}

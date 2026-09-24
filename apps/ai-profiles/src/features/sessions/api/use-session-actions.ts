import type { MovePlan, SessionAction } from '@/lib/types'

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import {
  archiveSession,
  checkSessionAction,
  checkSessionRepair,
  moveSession,
  planSessionMove,
  repairSessions,
  restoreSession,
} from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'
import { retryUnlessNotInstalled } from '@/lib/query/retry'

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
 * What a move is asked to do.
 */
type MoveInput = {
  /**
   * The session to move.
   */
  sessionId: string
  /**
   * The profile to move it to — a managed profile's id, or `default:<app>`.
   */
  destinationId: string
  /**
   * Replace a copy at the destination that was used more recently.
   */
  replaceNewer: boolean
  /**
   * Quit the desktop apps in the way first.
   */
  quitApps: boolean
}

/**
 * What stands between a session and an action: a blocker, or the desktop app
 * that has to quit first. Looked up whenever it is asked for and again on
 * every window focus, as the user may close a terminal or quit the app
 * themselves while the question is open. A missing tool isn't retried.
 */
export function useSessionActionCheck(profileId: string, sessionId: string, action: SessionAction) {
  return useQuery({
    queryKey: queryKeys.sessionActionCheck(profileId, sessionId, action),
    queryFn: () => checkSessionAction(profileId, sessionId, action),
    staleTime: 0,
    gcTime: 0,
    refetchOnWindowFocus: 'always',
    retry: retryUnlessNotInstalled,
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

/**
 * What moving one of a profile's sessions to `destinationId` would do.
 * Looked up whenever it is asked for. Planning reads every profile's
 * sessions, so it is looked up again on window focus only while something
 * the user may clear meanwhile stands in the way: a terminal to close, or a
 * desktop app to quit. The move itself plans again before it starts. A
 * missing tool isn't retried.
 */
export function useSessionMovePlan(profileId: string, sessionId: string, destinationId: string) {
  return useQuery({
    queryKey: queryKeys.sessionMovePlan(profileId, sessionId, destinationId),
    queryFn: () => planSessionMove(profileId, sessionId, destinationId),
    staleTime: 0,
    gcTime: 0,
    refetchOnWindowFocus: (query) => (waitsOnUser(query.state.data) ? 'always' : false),
    retry: retryUnlessNotInstalled,
  })
}

/**
 * Whether `plan` waits on something the user can clear outside the app: a
 * blocker, or a desktop app to quit.
 */
function waitsOnUser(plan: MovePlan | undefined): boolean {
  return plan !== undefined && (plan.blockers.length > 0 || plan.appsToQuit.length > 0)
}

/**
 * Moves one of a profile's sessions to another profile. Every profile's list
 * is refetched afterwards, whether it worked or not, as a move changes two
 * lists and may quit desktop apps on the way.
 */
export function useMoveSession(profileId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ sessionId, destinationId, replaceNewer, quitApps }: MoveInput) =>
      moveSession(profileId, sessionId, destinationId, replaceNewer, quitApps),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.sessions.all })
    },
  })
}

/**
 * What stands between a profile's sessions that need repair and their
 * repair: the profile's desktop app, when it runs. Looked up whenever it is
 * asked for and again on every window focus, as the user may quit the app
 * themselves while the question is open.
 */
export function useSessionRepairCheck(profileId: string) {
  return useQuery({
    queryKey: queryKeys.sessionRepairCheck(profileId),
    queryFn: () => checkSessionRepair(profileId),
    staleTime: 0,
    gcTime: 0,
    refetchOnWindowFocus: 'always',
  })
}

/**
 * Repairs a profile's sessions that need it, quitting its desktop app first
 * if `quitApp`. Every profile's list is refetched afterwards, whether it
 * worked or not, as the transcripts leave another profile's folder.
 */
export function useRepairSessions(profileId: string) {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (quitApp: boolean) => repairSessions(profileId, quitApp),
    onSettled: () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.sessions.all })
    },
  })
}

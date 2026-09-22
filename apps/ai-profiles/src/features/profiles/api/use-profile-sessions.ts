import type { TransferRequest } from '@/lib/types'

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { archiveSession, checkSessionArchive, listSessions, planSessionTransfer, transferSession } from '@/lib/commands'
import { queryKeys } from '@/lib/query/keys'

/**
 * The Claude sessions profile `id` keeps, newest first. Reads every
 * transcript, so it is fetched on mount and when the window regains focus
 * rather than polled: what changes it (a session started, an app quit so a
 * session can move) happens outside ai-profiles.
 */
export function useProfileSessions(id: string) {
  return useQuery({
    queryKey: queryKeys.sessions.list(id),
    queryFn: () => listSessions(id),
    refetchOnWindowFocus: 'always',
  })
}

/**
 * What moving a session would do. Re-read when the window regains focus, since
 * what blocks a move (an open app, a running session) is changed outside
 * ai-profiles.
 */
export function useTransferPlan(request: TransferRequest | null) {
  return useQuery({
    queryKey: queryKeys.sessions.transferPlan(request ?? {}),
    queryFn: () => planSessionTransfer(request as TransferRequest),
    enabled: request !== null,
    refetchOnWindowFocus: 'always',
    gcTime: 0,
  })
}

/**
 * Moves a session, then refreshes every profile's session list: both ends of
 * the move have changed.
 */
export function useTransferSession() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (request: TransferRequest) => transferSession(request),
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.sessions.all }),
  })
}

/**
 * What archiving a session needs first. Re-read on focus, like the transfer
 * plan: the app it may need quit is quit or reopened outside ai-profiles.
 */
export function useArchiveCheck(profileId: string, sessionId: string) {
  return useQuery({
    queryKey: queryKeys.sessions.archiveCheck(profileId, sessionId),
    queryFn: () => checkSessionArchive({ profileId, sessionId }),
    refetchOnWindowFocus: 'always',
    gcTime: 0,
  })
}

/**
 * Takes a session out of a profile, keeping it in session-transfer-backups.
 */
export function useArchiveSession() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: { profileId: string; sessionId: string; quitApp: boolean }) => archiveSession(input),
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.sessions.all }),
  })
}

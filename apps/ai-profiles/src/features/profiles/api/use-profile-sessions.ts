import type { TransferRequest } from '@/lib/types'

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import {
  archiveSession,
  checkSessionArchive,
  checkSessionRestore,
  listArchivedSessions,
  listSessions,
  planSessionTransfer,
  restoreSession,
  transferSession,
} from '@/lib/commands'
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

/**
 * The sessions profile `id` has archived, newest first.
 */
export function useArchivedSessions(id: string) {
  return useQuery({
    queryKey: queryKeys.sessions.archived(id),
    queryFn: () => listArchivedSessions(id),
    refetchOnWindowFocus: 'always',
  })
}

/**
 * What restoring an archived session needs first. Re-read on focus, like the
 * archive check.
 */
export function useRestoreCheck(profileId: string, sessionId: string, archive: string) {
  return useQuery({
    queryKey: queryKeys.sessions.restoreCheck(profileId, sessionId, archive),
    queryFn: () => checkSessionRestore({ profileId, sessionId, archive }),
    refetchOnWindowFocus: 'always',
    gcTime: 0,
  })
}

/**
 * Puts an archived session back, then refreshes the session lists.
 */
export function useRestoreSession() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: { profileId: string; sessionId: string; archive: string; quitApp: boolean }) =>
      restoreSession(input),
    onSettled: () => queryClient.invalidateQueries({ queryKey: queryKeys.sessions.all }),
  })
}

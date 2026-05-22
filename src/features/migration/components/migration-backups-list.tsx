import type { MigrationBackupInfo } from '@/lib/types'

import { useState } from 'react'

import { Button } from '@/design/ui/button'

type Props = {
  backups: Array<MigrationBackupInfo>
  onDelete: (path: string) => Promise<void>
}

function formatSize(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`
  }
  if (bytes < 1024 * 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`
  }
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`
}

function formatAge(timestampMs: number, nowMs: number): string {
  const deltaMs = nowMs - timestampMs
  const days = Math.floor(deltaMs / (24 * 60 * 60 * 1000))
  if (days === 0) {
    const hours = Math.floor(deltaMs / (60 * 60 * 1000))
    if (hours <= 0) {
      return 'just now'
    }
    return `${hours}h ago`
  }
  if (days === 1) {
    return '1 day ago'
  }
  return `${days} days ago`
}

export function MigrationBackupsList({ backups, onDelete }: Props) {
  const [busyPath, setBusyPath] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const now = Date.now()

  async function handleDelete(path: string) {
    setBusyPath(path)
    setError(null)
    try {
      await onDelete(path)
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : String(caught))
    } finally {
      setBusyPath(null)
    }
  }

  if (backups.length === 0) {
    return <p className="text-sm text-muted-foreground">No migration backups on this Mac.</p>
  }

  return (
    <div className="space-y-2">
      {backups.map((backup) => (
        <div
          key={backup.path}
          className="flex items-start justify-between gap-3 rounded-md border border-border bg-background p-3"
        >
          <div className="min-w-0 flex-1">
            <p className="truncate font-mono text-xs">{backup.path}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              {formatAge(backup.createdAtMs, now)} · {formatSize(backup.sizeBytes)}
              {backup.eligibleForCleanup ? (
                <span className="ml-2 rounded-sm bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium text-amber-900">
                  ready to delete
                </span>
              ) : null}
            </p>
          </div>
          <Button
            size="sm"
            variant="ghost"
            onClick={() => handleDelete(backup.path)}
            disabled={busyPath === backup.path}
          >
            Delete
          </Button>
        </div>
      ))}
      {error ? <p className="text-sm text-red">{error}</p> : null}
    </div>
  )
}

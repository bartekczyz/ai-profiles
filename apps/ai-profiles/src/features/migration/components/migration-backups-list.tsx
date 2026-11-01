import type { MigrationBackupInfo } from '@/lib/types'

import { useState } from 'react'

import { Folder } from '@phosphor-icons/react'

import { Button, cn, panelClasses } from '@/design'
import { formatBytes } from '@/lib/format-bytes'

type Props = {
  backups: Array<MigrationBackupInfo>
  onDelete: (path: string) => Promise<void>
}

function basename(path: string): string {
  const trimmed = path.endsWith('/') ? path.slice(0, -1) : path
  const lastSlash = trimmed.lastIndexOf('/')
  return lastSlash === -1 ? trimmed : trimmed.slice(lastSlash + 1)
}

function formatAge(timestampMs: number, nowMs: number): string {
  const deltaMs = nowMs - timestampMs
  const days = Math.floor(deltaMs / (24 * 60 * 60 * 1000))
  if (days === 0) {
    const hours = Math.floor(deltaMs / (60 * 60 * 1000))
    if (hours <= 0) {
      return 'just now'
    }
    return hours === 1 ? '1 hour old' : `${hours} hours old`
  }
  if (days === 1) {
    return '1 day old'
  }
  return `${days} days old`
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
    return (
      <p className={cn(panelClasses, 'px-4 py-3 text-[12.5px] tracking-[-0.003em] text-muted')}>
        No migration backups on this Mac.
      </p>
    )
  }

  return (
    <div className="flex flex-col gap-2">
      <div className={cn(panelClasses, 'divide-y divide-border-soft overflow-hidden')}>
        {backups.map((backup) => (
          <div key={backup.path} className="flex items-center gap-3 px-4 py-2.5">
            <span aria-hidden className="grid h-[22px] w-[22px] shrink-0 place-items-center text-muted-strong">
              <Folder className="h-4 w-4" />
            </span>
            <span className="min-w-0 flex-1 truncate font-mono text-[12px] text-ink-soft">{basename(backup.path)}</span>
            <span className="text-[11.5px] tracking-[-0.003em] text-muted">{formatAge(backup.createdAtMs, now)}</span>
            <span className="text-[12px] text-muted tabular-nums">{formatBytes(backup.sizeBytes)}</span>
            <Button
              size="sm"
              variant="danger"
              shortcut="Mod+Backspace"
              disabled={busyPath === backup.path}
              onClick={() => void handleDelete(backup.path)}
            >
              Delete
            </Button>
          </div>
        ))}
      </div>
      {error ? (
        <p role="alert" className="text-[11.5px] text-red">
          {error}
        </p>
      ) : null}
    </div>
  )
}

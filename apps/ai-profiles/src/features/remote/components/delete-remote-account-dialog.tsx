import type { RemoteAccount, RemoteHost } from '@/lib/types'

import { useEffect } from 'react'

import { Button, Dialog, Kbd, useToast } from '@/design'
import { sessionErrorMessage } from '@/features/profiles/components/session-error-message'

import { useDeleteRemoteAccount } from '../api/use-remote'

type Props = {
  open: boolean
  host: RemoteHost
  account: RemoteAccount
  onClose: () => void
  onDeleted: () => void
}

/**
 * Deleting a profile on a remote host (its account there). Nothing is erased: its folder moves
 * into the accounts folder's `.trash`, sessions and sign-in included, and
 * can be moved back by hand. The host refuses while a session of it is
 * running or it's being signed in.
 */
export function DeleteRemoteAccountDialog({ open, host, account, onClose, onDeleted }: Props) {
  const remove = useDeleteRemoteAccount(host.id)
  const toast = useToast()

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset each time the dialog opens
  useEffect(() => {
    if (open) {
      remove.reset()
    }
  }, [open])

  async function handleDelete() {
    if (remove.isPending) {
      return
    }
    try {
      const deleted = await remove.mutateAsync(account.name)
      toast.success(`Deleted ${account.name}`, `Moved to ${deleted.trashedTo} on ${host.label}.`)
      onClose()
      onDeleted()
    } catch {
      // Shown below.
    }
  }

  return (
    <Dialog
      open={open}
      title={`Delete ${account.name}?`}
      description={`On ${host.label}.`}
      onClose={onClose}
      foot={
        <>
          <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={remove.isPending} onClick={onClose}>
            Cancel
          </Button>
          <Button variant="danger" size="sm" disabled={remove.isPending} onClick={handleDelete}>
            {remove.isPending ? 'Deleting…' : 'Delete'}
          </Button>
        </>
      }
    >
      <div className="space-y-3 text-body text-ink-soft">
        <p>
          Its folder, with its sign-in and all its sessions, moves to{' '}
          <code className="font-mono text-[12px]">.trash</code> next to it. Nothing is erased: move it back to have the
          account again.
        </p>
        <p className="truncate font-mono text-[11.5px] text-muted" title={account.configDir}>
          {account.configDir}
        </p>
        {remove.isError ? (
          <p role="alert" className="text-meta text-red">
            {sessionErrorMessage(remove.error, 'The profile could not be deleted.')}
          </p>
        ) : null}
      </div>
    </Dialog>
  )
}

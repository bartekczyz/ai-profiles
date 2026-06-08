import { ariaKeyshortcutsFor, Kbd } from '@/design'

type Props = {
  onDelete: () => void
}

/**
 * Subdued danger affordance — a text-only red link with a ⌘⌫ chip and a
 * muted helper to the right. Clicking opens the existing confirm dialog;
 * the actual destruction still requires explicit confirmation there.
 */
export function ProfileDetailDangerLink({ onDelete }: Props) {
  return (
    <div className="flex items-center gap-2.5 text-meta text-muted">
      <button
        type="button"
        onClick={onDelete}
        aria-keyshortcuts={ariaKeyshortcutsFor('delete-selected')}
        className="inline-flex cursor-pointer items-center gap-1.5 border-0 bg-transparent p-0 font-medium text-red border-b border-dashed border-red/35 pb-px outline-none hover:text-red hover:border-red/65"
      >
        <span>Delete this profile</span>
        <Kbd variant="subtle" shortcutId="delete-selected" />
      </button>
      <span className="text-muted-strong">Removes launchers, keeps the isolated data on disk.</span>
    </div>
  )
}

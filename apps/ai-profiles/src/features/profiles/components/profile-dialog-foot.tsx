import { Button, Kbd } from '@/design'

type ProfileDialogFootProps = {
  /**
   * Whether the form's current values are submittable.
   */
  canSubmit: boolean
  /**
   * Primary button label while idle, e.g. "Create profile".
   */
  submitLabel: string
  /**
   * Primary button label while `submitting`, e.g. "Creating…".
   */
  submittingLabel: string
  /**
   * Whether a submit is in flight; disables both buttons.
   */
  submitting?: boolean
  /**
   * Closes the dialog without saving.
   */
  onCancel: () => void
  /**
   * Submits the form.
   */
  onSubmit: () => void
}

/**
 * Cancel + submit buttons shared by the create and edit profile dialogs.
 */
export function ProfileDialogFoot({
  canSubmit,
  submitLabel,
  submittingLabel,
  submitting = false,
  onCancel,
  onSubmit,
}: ProfileDialogFootProps) {
  return (
    <>
      <Button variant="ghost" size="sm" trailingKbd={<Kbd>⎋</Kbd>} disabled={submitting} onClick={onCancel}>
        Cancel
      </Button>
      <Button
        variant="primary"
        size="sm"
        trailingKbd={<Kbd variant="onOrange">⏎</Kbd>}
        disabled={!canSubmit || submitting}
        onClick={onSubmit}
      >
        {submitting ? submittingLabel : submitLabel}
      </Button>
    </>
  )
}

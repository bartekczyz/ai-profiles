type Props = {
  onMigrate: () => void
}

/**
 * Bottom-of-detail action for the default entry — replaces the
 * "Delete this profile" danger link with a "Migrate this profile to a
 * managed profile" action that opens the existing migration dialog.
 * Neutral colour (the action is constructive, not destructive).
 */
export function ProfileDetailMigrateLink({ onMigrate }: Props) {
  return (
    <div className="flex items-center gap-2.5 text-meta text-muted">
      <button
        type="button"
        onClick={onMigrate}
        className="inline-flex cursor-pointer items-center gap-1.5 border-0 bg-transparent p-0 font-medium text-ink border-b border-dashed border-border-strong pb-px outline-none hover:text-ink hover:border-ink/65"
      >
        <span>Migrate this profile to a managed profile</span>
      </button>
    </div>
  )
}

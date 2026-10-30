import { toolbarControlClasses } from './profile-detail-header'

type Props = {
  onMigrate: () => void
}

/**
 * The stock install's one unique action: bring this install under
 * management. It leads the header's action group, taking the seat and the
 * chrome a managed profile gives to Edit, so the two panes read as one
 * screen whose controls differ by capability rather than as two designs.
 *
 * It replaces the dashed "Migrate this profile to a managed profile" link
 * that used to sit under the pane. The redesigned pane has exactly one
 * container system — header, inset panels, status line — and a bare
 * underlined sentence at the foot belonged to none of it; sitting in the
 * header also puts the action above the fold rather than below the usage
 * meters, whose height moves.
 *
 * The trailing ellipsis follows the platform convention the menu already
 * uses ("Delete profile…"): the click opens a dialog, it does not import
 * anything on its own. No keyboard chip — ⌘I opens the migration dialog
 * globally, but for the first importable app rather than this entry's, so
 * announcing it here would be a lie.
 */
export function ProfileDetailMigrateAction({ onMigrate }: Props) {
  return (
    <button type="button" className={toolbarControlClasses} onClick={onMigrate}>
      Import…
    </button>
  )
}

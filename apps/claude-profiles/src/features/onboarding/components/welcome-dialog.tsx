import { Button, Dialog, Kbd } from '@/design'

type Props = {
  open: boolean
  onContinue: () => void
}

export function WelcomeDialog({ open, onContinue }: Props) {
  return (
    <Dialog
      open={open}
      title="Welcome to claude-profiles"
      description="Run multiple Anthropic Claude accounts on one Mac — the desktop app and the Claude Code CLI, side by side."
      onClose={onContinue}
      onSubmit={onContinue}
      foot={
        <Button variant="primary" size="sm" trailingKbd={<Kbd variant="onOrange">⏎</Kbd>} onClick={onContinue}>
          Continue
        </Button>
      }
    >
      <p className="text-body text-ink-soft">
        Each profile keeps its own login, history, MCP config, and project memory. Launch them from the menu bar or run{' '}
        <code className="font-mono text-mono text-ink">claude-&lt;slug&gt;</code> in any terminal.
      </p>
    </Dialog>
  )
}

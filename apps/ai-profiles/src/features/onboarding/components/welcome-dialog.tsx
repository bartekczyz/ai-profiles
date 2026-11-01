import { Button, Dialog } from '@/design'

type Props = {
  open: boolean
  onContinue: () => void
}

export function WelcomeDialog({ open, onContinue }: Props) {
  return (
    <Dialog
      open={open}
      title="Welcome to ai-profiles"
      description="Run multiple isolated Claude and ChatGPT accounts on one Mac — desktop apps and CLIs, side by side."
      onClose={onContinue}
      onSubmit={onContinue}
      foot={
        <Button variant="primary" size="sm" shortcut="Enter" onClick={onContinue}>
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

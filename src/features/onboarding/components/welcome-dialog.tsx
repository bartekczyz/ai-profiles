import { Button } from '@/design/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/design/ui/dialog'

type Props = {
  open: boolean
  onContinue: () => void
}

export function WelcomeDialog({ open, onContinue }: Props) {
  return (
    <Dialog open={open}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Welcome to claude-profiles</DialogTitle>
          <DialogDescription>
            Run multiple Anthropic Claude accounts on one Mac — the desktop app and the Claude Code CLI, side by side.
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button onClick={onContinue}>Continue</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

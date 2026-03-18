import { Button } from '@/components/ui/button'

type ActiveTipToolbarButtonProps = {
  canGoToActiveTip: boolean
  onGoToActiveTip: () => void
  buttonClassName?: string
}

export function ActiveTipToolbarButton({
  canGoToActiveTip,
  onGoToActiveTip,
  buttonClassName,
}: ActiveTipToolbarButtonProps) {
  const activeTipButtonTitle = canGoToActiveTip ? 'Go to highest active tip' : 'No active tip in retained history yet'

  return (
    <span title={activeTipButtonTitle}>
      <Button
        type="button"
        size="xs"
        variant="outline"
        className={buttonClassName}
        onClick={onGoToActiveTip}
        disabled={!canGoToActiveTip}
        aria-label={canGoToActiveTip ? 'Go to active tip' : activeTipButtonTitle}
      >
        Go to Active Tip
      </Button>
    </span>
  )
}

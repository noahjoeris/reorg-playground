import { Panel } from '@xyflow/react'
import { Button } from '@/components/ui/button'
import { TriggerReorgButton } from './TriggerReorgButton'
import type { Network, NodeInfo } from './types'

const TOOLBAR_BUTTON = 'rounded-full border-border/80 bg-card/90 px-3 font-semibold backdrop-blur'

export function GraphToolbar({
  network,
  allNodes,
  showFoldToggle,
  globalCollapsed,
  onToggleGlobalCollapsed,
}: {
  network: Network | null
  allNodes: NodeInfo[]
  showFoldToggle: boolean
  globalCollapsed: boolean
  onToggleGlobalCollapsed: () => void
}) {
  return (
    <Panel position="top-right" className="m-2">
      <div className="flex items-center gap-1.5">
        {network && <TriggerReorgButton network={network} nodes={allNodes} buttonClassName={TOOLBAR_BUTTON} />}
        {showFoldToggle && (
          <Button
            type="button"
            size="xs"
            variant="outline"
            className={TOOLBAR_BUTTON}
            onClick={onToggleGlobalCollapsed}
            aria-label={globalCollapsed ? 'Expand all folded blocks' : 'Collapse uninteresting blocks'}
            title={globalCollapsed ? 'Expand all' : 'Collapse all'}
          >
            {globalCollapsed ? 'Expand all' : 'Collapse all'}
          </Button>
        )}
      </div>
    </Panel>
  )
}

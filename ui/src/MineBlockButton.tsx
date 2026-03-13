import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useMineBlock } from '@/hooks/useMineBlock'
import { cn } from '@/utils'
import type { Network, NodeInfo, ProcessedBlock } from './types'

type MineBlockButtonProps = {
  block: ProcessedBlock
  network: Network
  nodes: NodeInfo[]
  label?: string
  buttonClassName?: string
}

export function MineBlockButton({
  block,
  network,
  nodes,
  label = 'Mine Block',
  buttonClassName,
}: MineBlockButtonProps) {
  const activeTip = block.tipStatuses.find(tipStatus => tipStatus.status === 'active')
  const activeTipNodeNames = new Set(activeTip?.nodeNames ?? [])
  // Mining should only target nodes that currently consider this block their active tip.
  const activeTipNodes = nodes.filter(node => activeTipNodeNames.has(node.name))

  const {
    mine,
    loading,
    isFeatureEnabled: miningControlFeatureEnabled,
    isEnabledByNodeId: miningIsEnabledByNodeId,
  } = useMineBlock(network, activeTipNodes)
  const [dialogOpen, setDialogOpen] = useState(false)

  if (!activeTip) return null

  const mineEnabledNodes = activeTipNodes.filter(node => miningIsEnabledByNodeId[node.id] ?? false)

  if (mineEnabledNodes.length === 0) return null

  const handleMine = async (node: NodeInfo) => {
    await mine(node)
    setDialogOpen(false)
  }

  const handleClick = () => {
    if (mineEnabledNodes.length === 1) {
      void handleMine(mineEnabledNodes[0])
    } else {
      setDialogOpen(true)
    }
  }

  return (
    <>
      <Button
        variant="outline"
        size="xs"
        className={cn('w-full rounded-full', buttonClassName)}
        onClick={(e: React.MouseEvent) => {
          e.stopPropagation()
          handleClick()
        }}
        disabled={loading || !miningControlFeatureEnabled}
      >
        {loading ? 'Mining...' : label}
      </Button>

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent onClick={(e: React.MouseEvent) => e.stopPropagation()}>
          <DialogHeader>
            <DialogTitle>Select Node to Mine</DialogTitle>
            <DialogDescription>
              Multiple nodes have block #{block.height} as their active tip. Choose which node should mine the next
              block.
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-2 pt-2">
            {mineEnabledNodes.map(node => (
              <Button
                key={node.id}
                variant="outline"
                className="w-full justify-start"
                onClick={() => handleMine(node)}
                disabled={loading || !(miningIsEnabledByNodeId[node.id] ?? false)}
              >
                {loading ? 'Mining...' : node.name}
                <span className="ml-auto text-xs text-muted-foreground">{node.description}</span>
              </Button>
            ))}
          </div>
        </DialogContent>
      </Dialog>
    </>
  )
}

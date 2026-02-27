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
  const activeEntry = block.tipStatuses.find(ts => ts.status === 'active')
  const activeNodeNames = new Set(activeEntry?.nodeNames ?? [])
  const candidateActiveNodes = nodes.filter(n => activeNodeNames.has(n.name))

  const {
    mine,
    loading,
    error,
    featureEnabled: miningControlFeatureEnabled,
    isEnabledByNodeId: miningIsEnabledByNodeId,
  } = useMineBlock(network, candidateActiveNodes)
  const [dialogOpen, setDialogOpen] = useState(false)

  if (!activeEntry) return null

  const enabledNodes = candidateActiveNodes.filter(node => miningIsEnabledByNodeId[node.id] ?? false)

  if (enabledNodes.length === 0) return null

  const handleMine = async (node: NodeInfo) => {
    await mine(node)
    setDialogOpen(false)
  }

  const handleClick = () => {
    if (enabledNodes.length === 1) {
      handleMine(enabledNodes[0])
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

      {error && <p className="mt-0.5 text-center text-[10px] text-destructive">{error}</p>}

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
            {enabledNodes.map(node => (
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

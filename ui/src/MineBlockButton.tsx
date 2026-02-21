import { useState } from 'react'
import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { cn } from '@/utils'
import { mineBlock } from './api'
import type { NodeInfo, ProcessedBlock } from './types'

type MineBlockButtonProps = {
  block: ProcessedBlock
  networkId: number
  nodes: NodeInfo[]
  label?: string
  buttonClassName?: string
}

export function MineBlockButton({
  block,
  networkId,
  nodes,
  label = 'Mine Block',
  buttonClassName,
}: MineBlockButtonProps) {
  const [loading, setLoading] = useState(false)
  const [dialogOpen, setDialogOpen] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const activeEntry = block.tipStatuses.find(ts => ts.status === 'active')
  if (!activeEntry) return null

  const activeNodeNames = activeEntry.nodeNames
  const activeNodes = nodes.filter(n => activeNodeNames.includes(n.name) && n.implementation === 'Bitcoin Core')

  if (activeNodes.length === 0) return null

  const handleMine = async (nodeId: number) => {
    setLoading(true)
    setError(null)
    try {
      const result = await mineBlock(networkId, nodeId)
      if (!result.success) {
        setError(result.error ?? 'Unknown error')
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Network error')
    } finally {
      setLoading(false)
      setDialogOpen(false)
    }
  }

  const handleClick = () => {
    if (activeNodes.length === 1) {
      handleMine(activeNodes[0].id)
    } else {
      setDialogOpen(true)
    }
  }

  return (
    <>
      <Button
        variant="outline"
        size="xs"
        className={cn('w-full rounded-full bg-accent/10 text-accent hover:bg-accent/20', buttonClassName)}
        onClick={(e: React.MouseEvent) => {
          e.stopPropagation()
          handleClick()
        }}
        disabled={loading}
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
            {activeNodes.map(node => (
              <Button
                key={node.id}
                variant="outline"
                className="w-full justify-start"
                onClick={() => handleMine(node.id)}
                disabled={loading}
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

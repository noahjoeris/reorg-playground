import { Handle, type Node, type NodeProps, Position } from '@xyflow/react'
import { memo } from 'react'

type FoldedBlockTreeNodeData = {
  startHeight: number
  endHeight: number
  hiddenCount: number
}

export type FoldedBlockTreeNodeType = Node<FoldedBlockTreeNodeData, 'folded'>

function FoldedBlockTreeNodeComponent({ data }: NodeProps<FoldedBlockTreeNodeType>) {
  return (
    <div className="group relative min-w-48 max-w-48">
      <Handle
        type="target"
        position={Position.Left}
        className="relative h-3 w-3 border-2 border-background bg-muted-foreground text-muted-foreground after:pointer-events-none after:absolute after:inset-[-0.22rem] after:rounded-full after:bg-current after:opacity-25 after:blur-[6px] after:content-['']"
      />

      <div className="flex h-24 w-full flex-col items-center justify-center gap-1 rounded-xl border border-dashed border-border/60 bg-muted/30 px-3 py-2 text-center backdrop-blur-sm transition-colors duration-200 ease-out">
        <p className="text-xs font-medium text-muted-foreground">{data.hiddenCount} blocks</p>
      </div>

      <Handle
        type="source"
        position={Position.Right}
        className="relative h-3 w-3 border-2 border-background bg-muted-foreground text-muted-foreground after:pointer-events-none after:absolute after:inset-[-0.22rem] after:rounded-full after:bg-current after:opacity-25 after:blur-[6px] after:content-['']"
      />
    </div>
  )
}

export const FoldedBlockTreeNode = memo(FoldedBlockTreeNodeComponent)
FoldedBlockTreeNode.displayName = 'FoldedBlockTreeNode'

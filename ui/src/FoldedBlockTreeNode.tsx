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
    <div className="group relative w-24">
      <Handle
        type="target"
        position={Position.Left}
        className="absolute! h-2.5 w-2.5 border-2 border-background bg-muted-foreground text-muted-foreground after:pointer-events-none after:absolute after:inset-[-0.22rem] after:rounded-full after:bg-current after:opacity-25 after:blur-[6px] after:content-['']"
      />

      <div className="flex min-h-16 w-full items-center justify-center rounded-sm border border-dashed border-border/60 bg-muted/30 px-2 text-center backdrop-blur-sm transition-colors duration-200 ease-out">
        <p className="text-[10px] font-medium text-muted-foreground">{data.hiddenCount} blocks</p>
      </div>

      <Handle
        type="source"
        position={Position.Right}
        className="absolute! h-2.5 w-2.5 border-2 border-background bg-muted-foreground text-muted-foreground after:pointer-events-none after:absolute after:inset-[-0.22rem] after:rounded-full after:bg-current after:opacity-25 after:blur-[6px] after:content-['']"
      />
    </div>
  )
}

export const FoldedBlockTreeNode = memo(FoldedBlockTreeNodeComponent)
FoldedBlockTreeNode.displayName = 'FoldedBlockTreeNode'

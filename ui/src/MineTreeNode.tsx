import { Handle, type Node, type NodeProps, Position } from '@xyflow/react'
import { memo } from 'react'
import { MineBlockButton } from './MineBlockButton'
import type { NodeInfo, ProcessedBlock } from './types'

type MineTreeNodeData = {
  block: ProcessedBlock
  networkId: number
  nodes: NodeInfo[]
}

export type MineTreeNodeType = Node<MineTreeNodeData, 'mine'>

function MineTreeNodeComponent({ data, selected }: NodeProps<MineTreeNodeType>) {
  return (
    <div className="group relative flex h-36 items-center">
      <Handle
        type="target"
        position={Position.Left}
        className={[
          "relative h-3 w-3 border-2 border-background after:pointer-events-none after:absolute after:inset-[-0.22rem] after:rounded-full after:bg-current after:opacity-25 after:blur-[6px] after:content-['']",
          selected ? 'bg-accent text-accent' : 'bg-muted-foreground text-muted-foreground',
        ].join(' ')}
      />

      <div className="pl-3">
        <MineBlockButton
          block={data.block}
          networkId={data.networkId}
          nodes={data.nodes}
          label="Mine block"
          buttonClassName={[
            'inline-flex h-7 w-auto px-3 text-[11px] font-semibold',
            selected ? 'ring-2 ring-accent/55 ring-offset-2 ring-offset-background' : '',
          ].join(' ')}
        />
      </div>
    </div>
  )
}

export const MineTreeNode = memo(MineTreeNodeComponent)
MineTreeNode.displayName = 'MineTreeNode'

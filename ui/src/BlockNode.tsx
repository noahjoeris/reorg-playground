import { Handle, type Node, type NodeProps, Position } from '@xyflow/react'
import type { TipStatusEntry } from './types'
import { TIP_STATUS_COLORS } from './types'

type BlockNodeData = {
  height: number
  hash: string
  miner: string
  tipStatuses: TipStatusEntry[]
  difficultyInt: number
  onBlockClick: () => void
}

export type BlockNodeType = Node<BlockNodeData, 'block'>

export function BlockNode({ data }: NodeProps<BlockNodeType>) {
  const truncatedMiner = data.miner ? data.miner.slice(0, 14) : ''
  const truncatedHash = `${data.hash.slice(0, 10)}...`
  const isLowDifficulty = data.difficultyInt === 1
  const hasTipStatus = data.tipStatuses.length > 0

  return (
    <div
      className={`relative min-w-40 cursor-pointer rounded-lg border bg-background px-4 py-3 shadow-sm transition-shadow hover:shadow-md ${
        isLowDifficulty ? 'border-[darksalmon]' : 'border-border'
      }`}
      onClick={data.onBlockClick}
    >
      <Handle type="target" position={Position.Left} />

      {hasTipStatus && (
        <div className="absolute -top-3 left-2 flex gap-1">
          {data.tipStatuses.map(ts => (
            <span
              key={ts.status}
              className="h-2.5 w-2.5 rounded-full"
              style={{ backgroundColor: TIP_STATUS_COLORS[ts.status] }}
              title={`${ts.status}: ${ts.nodeNames.join(', ')}`}
            />
          ))}
        </div>
      )}

      <div className="text-sm font-semibold text-foreground">#{data.height}</div>
      <div className="mt-0.5 font-mono text-xs text-muted-foreground">{truncatedHash}</div>
      {truncatedMiner && <div className="mt-0.5 text-xs text-muted-foreground">{truncatedMiner}</div>}

      <Handle type="source" position={Position.Right} />
    </div>
  )
}

import { Handle, type Node, type NodeProps, Position } from '@xyflow/react'
import { memo } from 'react'
import { TIP_STATUS_COLORS, type TipStatusEntry } from './types'
import { formatMinerLabel, shortHash } from './utils'

type BlockNodeData = {
  height: number
  hash: string
  miner: string
  tipStatuses: TipStatusEntry[]
  difficultyInt: number
  onBlockClick: () => void
}

export type BlockNodeType = Node<BlockNodeData, 'block'>

function BlockNodeComponent({ data, selected }: NodeProps<BlockNodeType>) {
  const truncatedHash = shortHash(data.hash, 10, 8)
  const minerLabel = formatMinerLabel(data.miner)
  const tipSummary = data.tipStatuses
    .map(tipStatus => `${tipStatus.status}: ${tipStatus.nodeNames.join(', ')}`)
    .join('\n')

  return (
    <div className="group relative min-w-56 max-w-56">
      <Handle
        type="target"
        position={Position.Left}
        className="h-2.5 w-2.5 border-2 border-background bg-muted-foreground"
      />

      <button
        type="button"
        onClick={data.onBlockClick}
        title={`Block #${data.height}`}
        aria-label={`Open details for block ${data.height}`}
        className={[
          'flex h-32 w-full flex-col rounded-xl border px-3 py-2.5 text-left transition duration-200',
          'bg-background/90 shadow-sm hover:-translate-y-0.5 hover:shadow-md',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent/70 focus-visible:ring-offset-2',
          selected
            ? 'border-accent shadow-[0_0_0_1px_var(--accent)] ring-2 ring-accent/30'
            : 'border-border/80 group-hover:border-accent/40',
        ].join(' ')}
      >
        <div className="flex items-start justify-between gap-2">
          <div>
            <p className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">Block</p>
            <p className="font-semibold text-foreground">#{data.height}</p>
          </div>
        </div>

        <p className="mt-1 font-mono text-xs text-foreground" title={data.hash}>
          {truncatedHash}
        </p>
        <p className="mt-1 truncate text-xs text-muted-foreground" title={minerLabel}>
          {minerLabel}
        </p>

        {data.tipStatuses.length > 0 && (
          <ul
            className="mt-auto flex max-w-full flex-nowrap gap-1.5 overflow-hidden pt-2"
            title={tipSummary}
            aria-label="Tip status overview"
          >
            {data.tipStatuses.map(tipStatus => (
              <li
                key={tipStatus.status}
                className="inline-flex shrink-0 items-center gap-1 rounded-full border border-border/80 bg-muted/70 px-1.5 py-0.5"
              >
                <span
                  className="h-1.5 w-1.5 rounded-full"
                  style={{ backgroundColor: TIP_STATUS_COLORS[tipStatus.status] }}
                />
                <span className="text-[10px] font-medium text-foreground">{tipStatus.status}</span>
                <span className="text-[10px] text-muted-foreground">{tipStatus.nodeNames.length}</span>
              </li>
            ))}
          </ul>
        )}
      </button>

      <Handle
        type="source"
        position={Position.Right}
        className="h-2.5 w-2.5 border-2 border-background bg-muted-foreground"
      />
    </div>
  )
}

export const BlockNode = memo(BlockNodeComponent)
BlockNode.displayName = 'BlockNode'

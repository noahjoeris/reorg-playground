import { Handle, type Node, type NodeProps, Position } from '@xyflow/react'
import { memo } from 'react'
import { TIP_STATUS_COLORS, type TipStatusEntry } from './types'
import { formatMinerLabel, shortHash } from './utils'

type BlockTreeNodeData = {
  height: number
  hash: string
  miner: string
  tipStatuses: TipStatusEntry[]
  onBlockClick: () => void
}

export type BlockTreeNodeType = Node<BlockTreeNodeData, 'block'>

function BlockTreeNodeComponent({ data, selected }: NodeProps<BlockTreeNodeType>) {
  const truncatedHash = shortHash(data.hash, 10, 8)
  const minerLabel = formatMinerLabel(data.miner)
  const tipSummary = data.tipStatuses
    .map(tipStatus => `${tipStatus.status}: ${tipStatus.nodeNames.join(', ')}`)
    .join('\n')

  return (
    <div className="group relative min-w-60 max-w-60">
      <Handle
        type="target"
        position={Position.Left}
        className={[
          "relative h-3 w-3 border-2 border-background after:pointer-events-none after:absolute after:inset-[-0.22rem] after:rounded-full after:bg-current after:opacity-25 after:blur-[6px] after:content-['']",
          selected ? 'bg-accent text-accent' : 'bg-muted-foreground text-muted-foreground',
        ].join(' ')}
      />

      <button
        type="button"
        onClick={data.onBlockClick}
        title={`Block #${data.height}`}
        aria-label={`Open details for block ${data.height}`}
        className={[
          'relative flex h-36 w-full flex-col overflow-hidden rounded-2xl border border-border/75 bg-muted/45 px-3.5 py-3 text-left dark:border-border/95 dark:bg-card/90',
          'shadow-[var(--elevation-soft)] backdrop-blur-md',
          'transition-[transform,border-color,box-shadow,background] duration-200 ease-out',
          'hover:-translate-y-0.5 hover:shadow-[var(--elevation-lift)]',
          'focus-visible:outline-none focus-visible:ring-[3px] focus-visible:ring-accent/70 focus-visible:ring-offset-2 focus-visible:ring-offset-background',
          selected
            ? 'border-accent/80 bg-accent/7 shadow-[0_0_0_1px_var(--accent),0_18px_40px_-20px_var(--surface-glow)]'
            : 'border-border/80 group-hover:border-accent/35',
        ].join(' ')}
      >
        <div className="flex items-start justify-between gap-2">
          <div>
            <p className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">Block</p>
            <p className="text-lg font-semibold leading-none text-foreground">#{data.height}</p>
          </div>
        </div>

        <p className="mt-2 font-mono text-xs text-foreground" title={data.hash}>
          {truncatedHash}
        </p>
        <p className="mt-1 truncate text-xs font-medium text-muted-foreground" title={minerLabel}>
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
                className="inline-flex shrink-0 items-center gap-1 rounded-full border border-border/80 bg-background/75 px-1.5 py-0.5"
              >
                <span
                  className="h-1.5 w-1.5 rounded-full ring-1 ring-background/70"
                  style={{ backgroundColor: TIP_STATUS_COLORS[tipStatus.status] }}
                />
                <span className="text-[10px] font-semibold text-foreground">{tipStatus.status}</span>
                <span className="rounded-full bg-muted px-1 text-[10px] text-muted-foreground">
                  {tipStatus.nodeNames.length}
                </span>
              </li>
            ))}
          </ul>
        )}
      </button>

      <Handle
        type="source"
        position={Position.Right}
        className={[
          "relative h-3 w-3 border-2 border-background after:pointer-events-none after:absolute after:inset-[-0.22rem] after:rounded-full after:bg-current after:opacity-25 after:blur-[6px] after:content-['']",
          selected ? 'bg-accent text-accent' : 'bg-muted-foreground text-muted-foreground',
        ].join(' ')}
      />
    </div>
  )
}

export const BlockTreeNode = memo(BlockTreeNodeComponent)
BlockTreeNode.displayName = 'BlockTreeNode'

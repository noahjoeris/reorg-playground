import { useEffect, useState } from 'react'
import type { NodeInfo } from './types'

function relativeTime(timestamp: number): string {
  const now = Math.floor(Date.now() / 1000)
  const diff = now - timestamp
  if (diff < 60) return `${diff}s ago`
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

function heightColor(height: number, maxHeight: number): string {
  if (maxHeight === 0) return 'hsl(0, 0%, 50%)'
  const ratio = height / maxHeight
  const hue = ratio * 120 // 0 = red, 120 = green
  return `hsl(${hue}, 70%, 50%)`
}

export function NodeInfoPanel({ nodes }: { nodes: NodeInfo[] }) {
  const [, setTick] = useState(0)

  // Update relative timestamps every 10s
  useEffect(() => {
    const interval = setInterval(() => setTick(t => t + 1), 10_000)
    return () => clearInterval(interval)
  }, [])

  // Sort by active tip height (descending)
  const sorted = [...nodes].sort((a, b) => {
    const aHeight = a.tips.find(t => t.status === 'active')?.height ?? 0
    const bHeight = b.tips.find(t => t.status === 'active')?.height ?? 0
    return bHeight - aHeight
  })

  const maxHeight = Math.max(...sorted.map(n => n.tips.find(t => t.status === 'active')?.height ?? 0), 0)

  return (
    <div className="flex gap-2 overflow-x-auto border-b border-border px-4 py-2">
      {sorted.map(node => {
        const activeHeight = node.tips.find(t => t.status === 'active')?.height ?? 0
        const activeHash = node.tips.find(t => t.status === 'active')?.hash ?? ''

        return (
          <div
            key={node.id}
            className="flex min-w-48 shrink-0 flex-col gap-1 rounded-lg border border-border bg-background p-3 shadow-sm"
          >
            <div className="flex items-center justify-between">
              <span className="text-sm font-semibold text-foreground">{node.name}</span>
              <span
                className={`h-2 w-2 rounded-full ${node.reachable ? 'bg-green-500' : 'bg-red-500'}`}
                title={node.reachable ? 'Reachable' : 'Unreachable'}
              />
            </div>

            <div className="flex items-center gap-1">
              <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {node.implementation}
                {node.version && ` ${node.version}`}
              </span>
            </div>

            {node.description && (
              <div className="truncate text-xs text-muted-foreground" title={node.description}>
                {node.description}
              </div>
            )}

            <div className="mt-1 flex items-center gap-2">
              <div
                className="h-1.5 flex-1 rounded-full"
                style={{ backgroundColor: heightColor(activeHeight, maxHeight) }}
              />
              <span className="font-mono text-xs text-foreground">{activeHeight}</span>
            </div>

            <div className="flex items-center justify-between">
              <span className="font-mono text-[10px] text-muted-foreground">
                {activeHash ? `${activeHash.slice(0, 12)}...` : 'N/A'}
              </span>
              <span className="text-[10px] text-muted-foreground">{relativeTime(node.last_changed_timestamp)}</span>
            </div>
          </div>
        )
      })}
    </div>
  )
}

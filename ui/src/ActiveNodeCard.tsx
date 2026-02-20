import { useEffect, useMemo, useState } from 'react'
import type { NodeInfo, TipStatus } from './types'
import { shortHash } from './utils'

const RELATIVE_TIME_REFRESH_MS = 10_000

const TIP_STATUS_LABELS: Record<TipStatus, string> = {
  active: 'Active',
  invalid: 'Invalid',
  'valid-fork': 'Valid Fork',
  'valid-headers': 'Valid Headers',
  'headers-only': 'Headers Only',
  unknown: 'Unknown',
}

function relativeTime(timestamp: number): string {
  const now = Math.floor(Date.now() / 1000)
  const diff = Math.max(0, now - timestamp)

  if (diff < 60) return `${diff}s ago`
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86_400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86_400)}d ago`
}

function formatAbsoluteTime(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleString()
}

function activeTip(node: NodeInfo) {
  return node.tips.find(tip => tip.status === 'active')
}

function tipStatusSummary(node: NodeInfo): Array<[TipStatus, number]> {
  const counts = new Map<TipStatus, number>()

  for (const tip of node.tips) {
    counts.set(tip.status, (counts.get(tip.status) ?? 0) + 1)
  }

  return [...counts.entries()].sort((a, b) => b[1] - a[1])
}

export function ActiveNodeInfoCard({ nodes }: { nodes: NodeInfo[] }) {
  const [, setTick] = useState(0)

  useEffect(() => {
    const interval = window.setInterval(() => setTick(tick => tick + 1), RELATIVE_TIME_REFRESH_MS)
    return () => window.clearInterval(interval)
  }, [])

  const sortedNodes = useMemo(() => {
    return [...nodes].sort((a, b) => {
      const aHeight = activeTip(a)?.height ?? 0
      const bHeight = activeTip(b)?.height ?? 0
      return bHeight - aHeight
    })
  }, [nodes])

  const maxHeight = useMemo(() => {
    return Math.max(0, ...sortedNodes.map(node => activeTip(node)?.height ?? 0))
  }, [sortedNodes])

  if (sortedNodes.length === 0) {
    return (
      <section className="border-b border-border/80 px-4 py-3 sm:px-6" aria-label="Node health panel">
        <div className="rounded-xl border border-dashed border-border/80 bg-muted/25 p-4">
          <p className="text-sm text-muted-foreground">No node status data yet.</p>
        </div>
      </section>
    )
  }

  return (
    <section className="border-b border-border/80 px-4 py-3 sm:px-6" aria-label="Node health panel">
      <div className="flex gap-3 overflow-x-auto pb-1">
        {sortedNodes.map(node => {
          const currentActiveTip = activeTip(node)
          const activeHeight = currentActiveTip?.height ?? 0
          const activeHash = currentActiveTip?.hash ?? ''
          const lag = Math.max(0, maxHeight - activeHeight)
          const progress = maxHeight === 0 ? 0 : Math.round((activeHeight / maxHeight) * 100)
          const statusSummary = tipStatusSummary(node)

          return (
            <article
              key={node.id}
              className={[
                'min-w-72 shrink-0 rounded-xl border p-3.5 shadow-sm transition-colors',
                node.reachable ? 'border-border/80 bg-background/90' : 'border-red-400/40 bg-red-500/5',
              ].join(' ')}
              aria-label={`Node ${node.name}`}
            >
              <header className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <h3 className="truncate text-sm font-semibold text-foreground" title={node.name}>
                    {node.name}
                  </h3>
                  <p className="mt-0.5 truncate text-xs text-muted-foreground" title={node.description}>
                    {node.description || 'No description'}
                  </p>
                </div>

                <span
                  className={[
                    'inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium',
                    node.reachable
                      ? 'border-emerald-500/40 bg-emerald-500/10 text-emerald-600'
                      : 'border-red-500/40 bg-red-500/10 text-red-600',
                  ].join(' ')}
                  title={node.reachable ? 'Node is reachable' : 'Node is unreachable'}
                >
                  <span
                    className={['h-1.5 w-1.5 rounded-full', node.reachable ? 'bg-emerald-500' : 'bg-red-500'].join(' ')}
                  />
                  {node.reachable ? 'Reachable' : 'Unreachable'}
                </span>
              </header>

              <div className="mt-2 flex flex-wrap items-center gap-1.5">
                <span className="rounded-full border border-border/80 bg-muted/60 px-2 py-0.5 text-[11px] text-muted-foreground">
                  {node.implementation}
                </span>
                {node.version && (
                  <span className="rounded-full border border-border/80 bg-muted/60 px-2 py-0.5 font-mono text-[11px] text-muted-foreground">
                    {node.version}
                  </span>
                )}
              </div>

              <div className="mt-3 grid grid-cols-3 gap-2">
                <div className="rounded-md border border-border/70 bg-muted/30 p-2">
                  <p className="text-[10px] uppercase tracking-wide text-muted-foreground">Height</p>
                  <p className="mt-1 font-mono text-xs text-foreground">{activeHeight || 'N/A'}</p>
                </div>
                <div className="rounded-md border border-border/70 bg-muted/30 p-2">
                  <p className="text-[10px] uppercase tracking-wide text-muted-foreground">Lag</p>
                  <p className="mt-1 font-mono text-xs text-foreground">{lag}</p>
                </div>
                <div className="rounded-md border border-border/70 bg-muted/30 p-2">
                  <p className="text-[10px] uppercase tracking-wide text-muted-foreground">Tips</p>
                  <p className="mt-1 font-mono text-xs text-foreground">{node.tips.length}</p>
                </div>
              </div>

              <div className="mt-3">
                <div className="mb-1 flex items-center justify-between text-[11px] text-muted-foreground">
                  <span>Chain progress</span>
                  <span>{progress}%</span>
                </div>
                <div className="h-1.5 rounded-full bg-muted">
                  <div
                    className={[
                      'h-1.5 rounded-full transition-all duration-300',
                      node.reachable ? 'bg-accent' : 'bg-red-500/60',
                    ].join(' ')}
                    style={{ width: `${progress}%` }}
                  />
                </div>
              </div>

              <div className="mt-3 flex items-center justify-between gap-2">
                <p
                  className="truncate font-mono text-[11px] text-muted-foreground"
                  title={activeHash || 'No active tip hash'}
                >
                  {activeHash ? shortHash(activeHash, 8, 8) : 'No active tip'}
                </p>
                <p
                  className="shrink-0 text-[11px] text-muted-foreground"
                  title={formatAbsoluteTime(node.last_changed_timestamp)}
                >
                  {relativeTime(node.last_changed_timestamp)}
                </p>
              </div>

              {statusSummary.length > 0 && (
                <ul className="mt-2 flex flex-wrap gap-1.5">
                  {statusSummary.map(([status, count]) => (
                    <li
                      key={status}
                      className="rounded-full border border-border/80 bg-muted/40 px-2 py-0.5 text-[10px] text-muted-foreground"
                    >
                      {TIP_STATUS_LABELS[status]}: {count}
                    </li>
                  ))}
                </ul>
              )}
            </article>
          )
        })}
      </div>
    </section>
  )
}

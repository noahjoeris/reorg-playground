import { useEffect, useMemo, useState } from 'react'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
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

function ReachabilityBadge({ reachable }: { reachable: boolean }) {
  return (
    <Badge
      variant={reachable ? 'secondary' : 'destructive'}
      className={[
        'ml-auto inline-flex max-w-full items-center gap-1.5 px-2 py-0.5 text-[11px]',
        reachable
          ? 'border-emerald-500/35 bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
          : 'border-red-500/35 bg-red-500/10 text-red-600 dark:text-red-400',
      ].join(' ')}
    >
      <span
        className={[
          'h-1.5 w-1.5 shrink-0 rounded-full',
          reachable ? 'bg-emerald-500' : 'bg-red-500',
        ].join(' ')}
        aria-hidden="true"
      />
      {reachable ? 'Reachable' : 'Unreachable'}
    </Badge>
  )
}

function NodeMetric({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="min-w-0">
      <p className="text-[10px] uppercase tracking-wide text-muted-foreground">{label}</p>
      <p className="mt-0.5 truncate font-mono text-sm text-foreground">{value}</p>
    </div>
  )
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
        <Card className="gap-0 border-dashed py-0">
          <CardContent className="p-4">
            <p className="text-sm text-muted-foreground">No node status data yet.</p>
          </CardContent>
        </Card>
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
            <Card
              key={node.id}
              className={[
                'min-w-72 shrink-0 gap-0 py-0 transition-colors',
                !node.reachable && 'border-red-400/40 bg-red-500/5',
              ]
                .filter(Boolean)
                .join(' ')}
              aria-label={`Node ${node.name}`}
            >
              <CardHeader className="gap-2 px-4 pt-4 pb-0">
                <div className="flex flex-wrap items-start gap-2">
                  <div className="min-w-0 flex-1">
                    <CardTitle className="truncate text-sm" title={node.name}>
                      {node.name}
                    </CardTitle>
                    <p className="mt-0.5 truncate text-xs text-muted-foreground" title={node.description}>
                      {node.description || 'No description'}
                    </p>
                  </div>
                  <ReachabilityBadge reachable={node.reachable} />
                </div>

                <div className="flex flex-wrap items-center gap-1.5">
                  <Badge variant="outline" className="font-normal text-muted-foreground">
                    {node.implementation}
                  </Badge>
                  {node.version && (
                    <Badge variant="outline" className="max-w-full truncate font-mono font-normal text-muted-foreground">
                      {node.version}
                    </Badge>
                  )}
                </div>
              </CardHeader>

              <CardContent className="space-y-3 px-4 py-3">
                <dl className="grid grid-cols-3 gap-3 border-y border-border/70 py-2">
                  <NodeMetric label="Height" value={activeHeight || 'N/A'} />
                  <NodeMetric label="Lag" value={lag} />
                  <NodeMetric label="Tips" value={node.tips.length} />
                </dl>

                <div>
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

                <div className="flex items-center justify-between gap-2">
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <p className="truncate font-mono text-[11px] text-muted-foreground">
                        {activeHash ? shortHash(activeHash, 8, 8) : 'No active tip'}
                      </p>
                    </TooltipTrigger>
                    <TooltipContent>{activeHash || 'No active tip hash'}</TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <p className="shrink-0 text-[11px] text-muted-foreground">
                        {relativeTime(node.last_changed_timestamp)}
                      </p>
                    </TooltipTrigger>
                    <TooltipContent>{formatAbsoluteTime(node.last_changed_timestamp)}</TooltipContent>
                  </Tooltip>
                </div>

                {statusSummary.length > 0 && (
                  <ul className="flex flex-wrap gap-1.5">
                    {statusSummary.map(([status, count]) => (
                      <li key={status}>
                        <Badge variant="outline" className="font-normal text-muted-foreground">
                          {TIP_STATUS_LABELS[status]} {count}
                        </Badge>
                      </li>
                    ))}
                  </ul>
                )}
              </CardContent>
            </Card>
          )
        })}
      </div>
    </section>
  )
}

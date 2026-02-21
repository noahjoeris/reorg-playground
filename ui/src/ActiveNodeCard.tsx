import { useEffect, useMemo, useState } from 'react'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { type NodeInfo, TIP_STATUS_COLORS, type TipStatus } from './types'
import { shortHash } from './utils'

const RELATIVE_TIME_REFRESH_MS = 10_000
const panelGlassClass =
  '[background:var(--surface-panel)] border border-border/70 shadow-[var(--elevation-soft)] backdrop-blur-[10px]'

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
        'ml-auto inline-flex max-w-full items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-semibold',
        reachable
          ? 'border-success/40 bg-success/12 text-success'
          : 'border-destructive/40 bg-destructive/12 text-destructive',
      ].join(' ')}
    >
      <span
        className={[
          "relative h-1.5 w-1.5 shrink-0 rounded-full after:pointer-events-none after:absolute after:inset-[-0.22rem] after:rounded-full after:bg-current after:opacity-25 after:blur-[6px] after:content-['']",
          reachable ? 'bg-success' : 'bg-destructive',
        ].join(' ')}
        aria-hidden="true"
      />
      {reachable ? 'Reachable' : 'Unreachable'}
    </Badge>
  )
}

function NodeMetric({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="inline-flex items-baseline gap-1 rounded-lg border border-border/75 bg-background/55 px-1.5 py-0.5">
      <p className="text-[10px] font-semibold uppercase tracking-widest text-muted-foreground">{label}</p>
      <p className="truncate font-mono text-xs font-semibold text-foreground">{value}</p>
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
      <section className="px-3 pb-1.5 sm:px-4 lg:px-5" aria-label="Node health panel">
        <Card className={`${panelGlassClass} gap-0 rounded-2xl border-dashed py-0`}>
          <CardContent className="p-3">
            <p className="text-sm text-muted-foreground">No node status data yet.</p>
          </CardContent>
        </Card>
      </section>
    )
  }

  return (
    <section className="px-3 pb-1.5 sm:px-4 lg:px-5" aria-label="Node health panel">
      <div className="flex gap-2.5 overflow-x-auto overscroll-x-contain pb-1.5">
        {sortedNodes.map(node => {
          const currentActiveTip = activeTip(node)
          const activeHeight = currentActiveTip?.height ?? 0
          const activeHash = currentActiveTip?.hash ?? ''
          const lag = Math.max(0, maxHeight - activeHeight)
          const statusSummary = tipStatusSummary(node)

          return (
            <Card
              key={node.id}
              className={[
                `${panelGlassClass} min-w-56 shrink-0 gap-0 rounded-2xl py-0 sm:min-w-64`,
                'transition-[transform,border-color,box-shadow] duration-200 ease-out hover:-translate-y-0.5',
                'hover:border-accent/35 hover:shadow-(--elevation-lift)',
                !node.reachable && 'border-destructive/40 bg-destructive/8',
              ]
                .filter(Boolean)
                .join(' ')}
              aria-label={`Node ${node.name}`}
            >
              <CardHeader className="gap-1 px-3 pt-2.5 pb-0">
                <div className="flex flex-wrap items-start gap-2">
                  <div className="min-w-0 flex-1">
                    <CardTitle className="truncate text-[13px] tracking-tight" title={node.name}>
                      {node.name}
                    </CardTitle>
                    <p
                      className="mt-0.5 hidden truncate text-xs font-medium text-muted-foreground sm:block"
                      title={node.description}
                    >
                      {node.description || 'No description'}
                    </p>
                  </div>
                  <ReachabilityBadge reachable={node.reachable} />
                </div>

                <div className="flex flex-wrap items-center gap-1.5">
                  <Badge variant="outline" className="rounded-full bg-background/65 font-normal text-muted-foreground">
                    {node.implementation}
                  </Badge>
                  {node.version && (
                    <Badge
                      variant="outline"
                      className="max-w-full truncate rounded-full bg-background/65 font-mono font-normal text-muted-foreground"
                    >
                      {node.version}
                    </Badge>
                  )}
                </div>
              </CardHeader>

              <CardContent className="space-y-1.5 px-3 py-2">
                <dl className="flex flex-wrap gap-1.5">
                  <NodeMetric label="Height" value={activeHeight || 'N/A'} />
                  <NodeMetric label="Lag" value={lag} />
                  <NodeMetric label="Tips" value={node.tips.length} />
                </dl>

                <div className="flex items-center justify-between gap-2">
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <p className="truncate rounded-md border border-border/70 bg-background/60 px-2 py-1 font-mono text-[11px] text-muted-foreground">
                        {activeHash ? shortHash(activeHash, 8, 8) : 'No active tip'}
                      </p>
                    </TooltipTrigger>
                    <TooltipContent>{activeHash || 'No active tip hash'}</TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <p className="shrink-0 rounded-md border border-border/70 bg-background/60 px-2 py-1 text-[11px] text-muted-foreground">
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
                        <Badge
                          variant="outline"
                          className="inline-flex items-center gap-1.5 rounded-full bg-background/65 font-normal text-muted-foreground"
                        >
                          <span
                            className="h-1.5 w-1.5 rounded-full ring-1 ring-background/70"
                            style={{ backgroundColor: TIP_STATUS_COLORS[status] }}
                            aria-hidden="true"
                          />
                          {TIP_STATUS_LABELS[status]}
                          <span className="rounded-full bg-muted px-1 text-[10px]">{count}</span>
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

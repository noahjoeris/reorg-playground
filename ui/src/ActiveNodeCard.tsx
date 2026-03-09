import { useEffect, useMemo, useState } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { useNodeP2PConnection } from '@/hooks/useNodeP2PConnection'
import {
  type Network,
  type NodeInfo,
  TIP_STATUS_COLORS,
  TIP_STATUS_DESCRIPTIONS,
  TIP_STATUS_LABELS,
  type TipStatus,
} from './types'

const RELATIVE_TIME_REFRESH_MS = 10_000
const panelGlassClass =
  '[background:var(--surface-panel)] border border-border/70 shadow-[var(--elevation-soft)] backdrop-blur-[10px]'
const compactBadgeClass = 'h-5 rounded-full bg-background/70 px-2 py-0.5 text-xs font-normal text-muted-foreground'
const P2P_RECONNECT_TOOLTIP = 'Reconnection can take ~30 seconds'

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
        'h-5 max-w-full rounded-full px-2 py-0.5 text-xs font-medium',
        reachable
          ? 'border-success/40 bg-success/10 text-success'
          : 'border-destructive/40 bg-destructive/10 text-destructive',
      ].join(' ')}
    >
      <span
        className={['size-2 rounded-full', reachable ? 'bg-success' : 'bg-destructive'].join(' ')}
        aria-hidden="true"
      />
      {reachable ? 'Reachable' : 'Unreachable'}
    </Badge>
  )
}

function NodeMetric({ label, value }: { label: string; value: string | number }) {
  return (
    <Badge variant="outline" className={compactBadgeClass}>
      <span>{label}</span>
      <span>{value}</span>
    </Badge>
  )
}

export function ActiveNodeInfoCard({ network, nodes }: { network: Network; nodes: NodeInfo[] }) {
  const [, setTick] = useState(0)
  const {
    toggleNodeP2PConnection,
    getNodeP2PConnectionActive,
    isEnabledByNodeId: p2pControlIsEnabledByNodeId,
    loadingByNodeId: p2pConnectionLoadingByNodeId,
    errorByNodeId: p2pConnectionErrorByNodeId,
  } = useNodeP2PConnection(network, nodes)

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
      <div className="flex flex-nowrap gap-2 overflow-x-auto overscroll-x-contain pb-1.5">
        {sortedNodes.map(node => {
          const currentActiveTip = activeTip(node)
          const activeHeight = currentActiveTip?.height ?? 0
          const lag = Math.max(0, maxHeight - activeHeight)
          const statusSummary = tipStatusSummary(node)
          const supportsNodeP2PControl = p2pControlIsEnabledByNodeId[node.id] ?? false
          const isP2PConnectionActive = getNodeP2PConnectionActive(node.id)
          const p2pConnectionLoading = p2pConnectionLoadingByNodeId[node.id] ?? false
          const p2pConnectionError = p2pConnectionErrorByNodeId[node.id]
          const p2pStatusUnknown = isP2PConnectionActive == null
          let p2pButtonLabel = 'Enable P2P'
          if (p2pConnectionLoading) {
            p2pButtonLabel = 'Updating...'
          } else if (p2pStatusUnknown) {
            p2pButtonLabel = 'Checking...'
          } else if (isP2PConnectionActive) {
            p2pButtonLabel = 'Disable P2P'
          }

          return (
            <Card
              key={node.id}
              className={[
                `${panelGlassClass} relative w-72 max-w-full shrink-0 gap-0 rounded-2xl py-0`,
                'transition duration-200 ease-out',
                'hover:border-accent/35 hover:shadow-(--elevation-lift)',
                !node.reachable && 'border-destructive/40 bg-destructive/10',
              ]
                .filter(Boolean)
                .join(' ')}
              aria-label={`Node ${node.name}`}
            >
              <CardHeader className="gap-1 px-3 pt-2.5 pb-0">
                <div className="flex items-start justify-between gap-1.5">
                  <CardTitle className="min-w-0 flex-1 truncate text-sm leading-tight" title={node.name}>
                    {node.name}
                  </CardTitle>
                  <ReachabilityBadge reachable={node.reachable} />
                </div>
                <CardDescription className="truncate text-xs font-medium" title={node.description || 'No description'}>
                  {node.description || 'No description'}
                </CardDescription>

                <div className="flex flex-wrap items-center gap-1">
                  <Badge variant="outline" className={compactBadgeClass}>
                    {node.implementation}
                  </Badge>
                  {node.version && (
                    <Badge variant="outline" className={`${compactBadgeClass} max-w-full truncate font-mono`}>
                      {node.version}
                    </Badge>
                  )}
                  <NodeMetric label="Height" value={activeHeight || 'N/A'} />
                  <NodeMetric label="Lag" value={lag} />
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Badge variant="outline" className={compactBadgeClass}>
                        <span>Updated</span>
                        <span>{relativeTime(node.last_changed_timestamp)}</span>
                      </Badge>
                    </TooltipTrigger>
                    <TooltipContent side="top">{formatAbsoluteTime(node.last_changed_timestamp)}</TooltipContent>
                  </Tooltip>
                </div>
              </CardHeader>

              <CardContent className="space-y-1.5 px-3 pt-2 pb-2.5">
                {supportsNodeP2PControl && (
                  <div className="space-y-1">
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <span className="block w-full">
                          <Button
                            type="button"
                            variant="outline"
                            size="xs"
                            className="w-full rounded-full"
                            onClick={() => void toggleNodeP2PConnection(node)}
                            disabled={p2pConnectionLoading || p2pStatusUnknown}
                          >
                            {p2pButtonLabel}
                          </Button>
                        </span>
                      </TooltipTrigger>
                      <TooltipContent side="top" className="max-w-64 text-xs leading-relaxed">
                        {P2P_RECONNECT_TOOLTIP}
                      </TooltipContent>
                    </Tooltip>
                    {p2pConnectionError && <p className="text-xs text-destructive">{p2pConnectionError}</p>}
                  </div>
                )}

                {statusSummary.length > 0 && (
                  <ul className="flex flex-wrap gap-1">
                    {statusSummary.map(([status, count]) => (
                      <li key={status} className="max-w-full">
                        <Tooltip>
                          <TooltipTrigger asChild>
                            <Badge
                              variant="outline"
                              className={`${compactBadgeClass} max-w-full justify-start text-left`}
                            >
                              <span
                                className="h-1.5 w-1.5 shrink-0 rounded-full ring-1 ring-background/70"
                                style={{ backgroundColor: TIP_STATUS_COLORS[status] }}
                                aria-hidden="true"
                              />
                              <span className="min-w-0 truncate">{TIP_STATUS_LABELS[status]}</span>
                              <span className="inline-flex size-4 shrink-0 items-center justify-center rounded-full bg-muted text-xs leading-none">
                                {count}
                              </span>
                            </Badge>
                          </TooltipTrigger>
                          <TooltipContent side="top" className="max-w-64">
                            {TIP_STATUS_DESCRIPTIONS[status]}
                          </TooltipContent>
                        </Tooltip>
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

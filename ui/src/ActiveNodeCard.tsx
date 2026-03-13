import { useMemo } from 'react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { type NodeInfo, TIP_STATUS_COLORS, TIP_STATUS_DESCRIPTIONS, TIP_STATUS_LABELS, type TipStatus } from './types'

const compactBadgeClass = 'h-5 rounded-full bg-background/70 px-2 py-0.5 text-xs font-normal text-muted-foreground'
const P2P_RECONNECT_TOOLTIP = 'Reconnection can take ~30 seconds'

export function activeTip(node: NodeInfo) {
  return node.tips.find(tip => tip.status === 'active')
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

export type P2PControl = {
  supported: boolean
  active: boolean | null
  loading: boolean
  onToggle: () => void
}

export function ActiveNodeCard({ node, maxHeight, p2p }: { node: NodeInfo; maxHeight: number; p2p: P2PControl }) {
  const activeHeight = activeTip(node)?.height ?? 0
  const lag = Math.max(0, maxHeight - activeHeight)
  const statusSummary = useMemo(() => tipStatusSummary(node), [node])

  const p2pStatusUnknown = p2p.active == null
  let p2pButtonLabel = 'Enable P2P'
  if (p2p.loading) {
    p2pButtonLabel = 'Updating...'
  } else if (p2pStatusUnknown) {
    p2pButtonLabel = 'Checking...'
  } else if (p2p.active) {
    p2pButtonLabel = 'Disable P2P'
  }

  return (
    <Card
      className={[
        'panel-glass relative w-72 max-w-full shrink-0 gap-0 rounded-2xl py-0',
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
        <Tooltip>
          <TooltipTrigger asChild>
            <CardDescription className="truncate text-xs font-medium">{node.description}</CardDescription>
          </TooltipTrigger>
          <TooltipContent side="top">{node.description}</TooltipContent>
        </Tooltip>

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
        {p2p.supported && (
          <div>
            <Tooltip>
              <TooltipTrigger asChild>
                <span className="block w-full">
                  <Button
                    type="button"
                    variant="outline"
                    size="xs"
                    className="w-full rounded-full"
                    onClick={p2p.onToggle}
                    disabled={p2p.loading || p2pStatusUnknown}
                  >
                    {p2pButtonLabel}
                  </Button>
                </span>
              </TooltipTrigger>
              <TooltipContent side="top" className="max-w-64 text-xs leading-relaxed">
                {P2P_RECONNECT_TOOLTIP}
              </TooltipContent>
            </Tooltip>
          </div>
        )}

        {statusSummary.length > 0 && (
          <ul className="flex flex-wrap gap-1">
            {statusSummary.map(([status, count]) => (
              <li key={status} className="max-w-full">
                <Tooltip>
                  <TooltipTrigger asChild>
                    <Badge variant="outline" className={`${compactBadgeClass} max-w-full justify-start`}>
                      <span
                        className="h-1.5 w-1.5 shrink-0 rounded-full ring-1 ring-background/70"
                        style={{ backgroundColor: TIP_STATUS_COLORS[status] }}
                        aria-hidden="true"
                      />
                      <span>{TIP_STATUS_LABELS[status]}</span>
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
}

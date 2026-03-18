import { Separator } from '@/components/ui/separator'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import { ConnectionStatus } from './ConnectionStatus'

const METRIC_PILL_CLASS = 'rounded-full border border-border/80 bg-card/70 px-2.5 py-px text-xs tracking-wide'
const STALE_RATE_GROUP_CLASS =
  'inline-flex flex-wrap items-center gap-1.5 rounded-2xl border border-amber-500/20 bg-amber-500/6 px-1.5 py-1 text-xs'
const STALE_RATE_GROUP_LABEL_CLASS =
  'rounded-full bg-amber-500/14 px-2.5 py-1 text-[10px] font-semibold uppercase tracking-[0.18em] text-amber-800 dark:text-amber-200'
const STALE_RATE_BADGE_CLASS =
  'rounded-full border border-amber-500/20 bg-background/80 px-2.5 py-1 text-left text-foreground shadow-sm shadow-black/5 transition-colors hover:border-amber-500/35 hover:bg-background'
const STALE_RATE_BADGE_UNAVAILABLE_CLASS =
  'border-border/70 bg-card/75 text-muted-foreground hover:border-border hover:bg-card'

type ConnectionState = 'connecting' | 'connected' | 'error' | 'closed'

export type MetricUnavailableReason = 'no_reachable_active_tip' | 'tip_not_in_tree' | 'insufficient_history'

export type StaleBlockRateRange = { kind: 'rolling'; blocks: number } | { kind: 'all_time' }

export type StaleBlockRateWindow = {
  range: StaleBlockRateRange
  stale_blocks: number
  active_blocks: number
  rate: number
  available: boolean
  reason?: MetricUnavailableReason
}

export type StaleBlockRate = {
  as_of_height: number | null
  windows: StaleBlockRateWindow[]
}

export type NetworkMetrics = {
  stale_block_rate: StaleBlockRate
}

const STALE_RATE_REASON_LABELS: Record<Exclude<MetricUnavailableReason, 'insufficient_history'>, string> = {
  no_reachable_active_tip: 'No reachable active tip is available yet.',
  tip_not_in_tree: 'The active tip is not present in retained history yet.',
}

function MetricsDivider() {
  return <Separator orientation="vertical" className="h-3.5 bg-border/70" />
}

function formatRate(rate: number) {
  return `${(rate * 100).toLocaleString(undefined, { minimumFractionDigits: 4, maximumFractionDigits: 4 })}%`
}

function rangeLabel(range: StaleBlockRateRange) {
  return range.kind === 'rolling' ? `${range.blocks.toLocaleString()} blocks` : 'All time'
}

function rangeDescription(range: StaleBlockRateRange) {
  return range.kind === 'rolling'
    ? `the last ${range.blocks.toLocaleString()} resolved blocks`
    : 'all retained resolved blocks'
}

function unavailableReasonLabel(range: StaleBlockRateRange, reason: MetricUnavailableReason) {
  if (reason !== 'insufficient_history') {
    return STALE_RATE_REASON_LABELS[reason]
  }

  return range.kind === 'rolling'
    ? 'Retained history does not fully cover this rolling window.'
    : 'Retained history is too shallow to resolve this metric.'
}

function metricTooltip(staleBlockRate: StaleBlockRate, window: StaleBlockRateWindow) {
  if (window.available) {
    return (
      <div className="space-y-2">
        <div className="space-y-0.5">
          <p className="font-medium">Observed stale rate</p>
          <p>Window: {rangeLabel(window.range)}</p>
        </div>
        <div className="space-y-0.5">
          <p>Rate: {formatRate(window.rate)}</p>
          <p>
            Breakdown: {window.stale_blocks.toLocaleString()} stale block{window.stale_blocks === 1 ? '' : 's'} and{' '}
            {window.active_blocks.toLocaleString()} active block{window.active_blocks === 1 ? '' : 's'} across{' '}
            {rangeDescription(window.range)}.
          </p>
        </div>
        {staleBlockRate.as_of_height !== null && (
          <p>Resolved anchor: height {staleBlockRate.as_of_height.toLocaleString()}.</p>
        )}
        <p className="text-background/80">
          This only counts stale branches that were retained and observed by this instance.
        </p>
      </div>
    )
  }

  return (
    <div className="space-y-2">
      <div className="space-y-0.5">
        <p className="font-medium">Observed stale rate unavailable</p>
        <p>Window: {rangeLabel(window.range)}</p>
      </div>
      <p>Why: {unavailableReasonLabel(window.range, window.reason ?? 'insufficient_history')}</p>
      {staleBlockRate.as_of_height !== null && (
        <p>Current resolved anchor: height {staleBlockRate.as_of_height.toLocaleString()}.</p>
      )}
      <p className="text-background/80">
        This metric needs a reachable resolved tip and retained history for the selected range.
      </p>
    </div>
  )
}

function StaleRateMetrics({ staleBlockRate }: { staleBlockRate: StaleBlockRate }) {
  return (
    <div className={STALE_RATE_GROUP_CLASS}>
      <span className={STALE_RATE_GROUP_LABEL_CLASS}>Stale rate</span>
      {staleBlockRate.windows.map(window => {
        const label = rangeLabel(window.range)
        const value = window.available ? formatRate(window.rate) : 'N/A'
        const ariaLabel = window.available
          ? `Observed stale rate for ${rangeDescription(window.range)}: ${value}`
          : `Observed stale rate for ${rangeDescription(window.range)} is unavailable`
        const key = window.range.kind === 'rolling' ? `rolling-${window.range.blocks}` : 'all-time'
        const className = window.available
          ? STALE_RATE_BADGE_CLASS
          : `${STALE_RATE_BADGE_CLASS} ${STALE_RATE_BADGE_UNAVAILABLE_CLASS}`
        const valueClass = window.available
          ? 'font-semibold tabular-nums text-foreground'
          : 'font-semibold tabular-nums text-muted-foreground'

        return (
          <Tooltip key={key}>
            <TooltipTrigger asChild>
              <button type="button" className={`${className} cursor-help`} aria-label={ariaLabel}>
                <span className="flex items-center gap-2">
                  <span className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
                    {label}
                  </span>
                  <span className="h-3 w-px bg-border/70" aria-hidden="true" />
                  <span className={valueClass}>{value}</span>
                </span>
              </button>
            </TooltipTrigger>
            <TooltipContent side="bottom" className="max-w-80">
              {metricTooltip(staleBlockRate, window)}
            </TooltipContent>
          </Tooltip>
        )
      })}
    </div>
  )
}

export function Metrics({
  blockCount,
  reachableNodes,
  totalNodes,
  metrics,
  connectionStatus,
}: {
  blockCount: number
  reachableNodes: number
  totalNodes: number
  metrics: NetworkMetrics | null
  connectionStatus: ConnectionState
}) {
  const staleBlockRate = metrics?.stale_block_rate ?? null

  return (
    <div className="mt-1.5 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
      <span className={METRIC_PILL_CLASS}>
        <span>{blockCount.toLocaleString()}</span> blocks
      </span>
      <MetricsDivider />
      <span className={METRIC_PILL_CLASS}>
        {reachableNodes}/{totalNodes} nodes reachable
      </span>
      {staleBlockRate && staleBlockRate.windows.length > 0 && (
        <>
          <MetricsDivider />
          <StaleRateMetrics staleBlockRate={staleBlockRate} />
        </>
      )}
      <MetricsDivider />
      <ConnectionStatus status={connectionStatus} />
    </div>
  )
}

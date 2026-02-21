import { Panel } from '@xyflow/react'
import { TIP_STATUS_COLORS, type TipStatus } from './types'

const LABELS: Record<TipStatus, string> = {
  active: 'Active',
  invalid: 'Invalid',
  'valid-fork': 'Valid Fork',
  'valid-headers': 'Valid Headers',
  'headers-only': 'Headers Only',
  unknown: 'Unknown',
}

export function Legend() {
  return (
    <Panel
      position="bottom-left"
      className="rounded-xl border border-border/70 [background:var(--surface-panel)] px-3.5 py-2.5 shadow-(--elevation-soft) backdrop-blur-[10px]"
    >
      <div className="flex flex-col gap-1.5">
        <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          Tip Status Key
        </span>
        {(Object.entries(TIP_STATUS_COLORS) as [TipStatus, string][]).map(([status, color]) => (
          <div
            key={status}
            className="flex items-center gap-2 rounded-md border border-border/70 bg-background/55 px-2 py-1"
          >
            <span className="h-2 w-2 rounded-full ring-1 ring-background/80" style={{ backgroundColor: color }} />
            <span className="text-xs font-medium text-foreground">{LABELS[status]}</span>
          </div>
        ))}
      </div>
    </Panel>
  )
}

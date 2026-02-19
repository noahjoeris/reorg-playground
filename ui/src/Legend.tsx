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
      className="rounded-lg border border-border bg-background/90 px-3 py-2 shadow-sm backdrop-blur-sm"
    >
      <div className="flex flex-col gap-1">
        <span className="text-[10px] font-semibold text-muted-foreground">Tip Status</span>
        {(Object.entries(TIP_STATUS_COLORS) as [TipStatus, string][]).map(([status, color]) => (
          <div key={status} className="flex items-center gap-2">
            <span className="h-2 w-2 rounded-full" style={{ backgroundColor: color }} />
            <span className="text-xs text-foreground">{LABELS[status]}</span>
          </div>
        ))}
      </div>
    </Panel>
  )
}

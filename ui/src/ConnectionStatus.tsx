import { Badge } from '@/components/ui/badge'
import type { ConnectionStatus as Status } from './types'

const CONFIG: Record<Status, { color: string; label: string }> = {
  connecting: { color: 'bg-warning', label: 'Connecting' },
  connected: { color: 'bg-success', label: 'Live' },
  error: { color: 'bg-destructive', label: 'Error' },
  closed: { color: 'bg-muted-foreground', label: 'Disconnected' },
}

export function ConnectionStatus({ status }: { status: Status }) {
  const { color, label } = CONFIG[status]

  return (
    <Badge variant="outline" className="gap-1.5 font-normal" role="status" aria-label={`Connection status: ${label}`}>
      <span className={`h-1.5 w-1.5 rounded-full ${color}`} />
      {label}
    </Badge>
  )
}
